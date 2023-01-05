// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_tests::{send_consensus, send_consensus_no_execution};
use crate::authority::{AuthorityState, EffectsNotifyRead};
use crate::authority_aggregator::authority_aggregator_tests::{
    create_object_move_transaction, do_cert, do_transaction, extract_cert, get_latest_ref,
    transfer_object_move_transaction,
};
use crate::safe_client::SafeClient;
use crate::test_authority_clients::LocalAuthorityClient;
use crate::test_utils::init_local_authorities;

use std::collections::BTreeSet;
use std::time::Duration;

use itertools::Itertools;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::messages::{TransactionEffects, VerifiedCertificate, VerifiedTransaction};
use sui_types::object::{Object, Owner};
use test_utils::messages::{make_counter_create_transaction, make_counter_increment_transaction};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

#[allow(dead_code)]
async fn wait_for_certs(
    stream: &mut UnboundedReceiver<VerifiedCertificate>,
    certs: &Vec<VerifiedCertificate>,
) {
    if certs.is_empty() {
        if timeout(Duration::from_secs(30), stream.recv())
            .await
            .is_err()
        {
            return;
        } else {
            panic!("Should not receive certificate!");
        }
    }
    let mut certs: BTreeSet<TransactionDigest> = certs.iter().map(|c| *c.digest()).collect();
    while !certs.is_empty() {
        match timeout(Duration::from_secs(5), stream.recv()).await {
            Err(_) => panic!("Timed out waiting for next certificate!"),
            Ok(None) => panic!("Next certificate channel closed!"),
            Ok(Some(cert)) => {
                println!("Found cert {:?}", cert.digest());
                certs.remove(cert.digest())
            }
        };
    }
}

/*
TODO: Re-enable after we have checkpoint v2.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_notify_ready_certificates() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();
    let mut ready_certificates_stream = authority_state.ready_certificates_stream().await.unwrap();

    // TODO: duplicated with checkpoint_driver/tests.rs
    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let active_state = Arc::new(
            ActiveAuthority::new_with_ephemeral_storage_for_test(
                inner_state.authority.clone(),
                inner_agg,
            )
            .unwrap(),
        );
        let _active_handle = active_state
            .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
            .await;
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.data().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Clear effects so their executions will happen below.
    authority_state
        .database
        .perpetual_tables
        .effects
        .clear()
        .expect("Clearing effects failed!");

    // Insert the certificates
    authority_state
        .enqueue_certificates_for_execution(certs.clone())
        .await
        .expect("Storage is ok");

    tokio::task::yield_now().await;

    // Wait to get back the certificates
    wait_for_certs(&mut ready_certificates_stream, &certs).await;

    // Should have no certificate any more.
    wait_for_certs(&mut ready_certificates_stream, &vec![]).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_full() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();

    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let _active_handle = tokio::task::spawn(async move {
            let active_state = Arc::new(
                ActiveAuthority::new_with_ephemeral_storage_for_test(
                    inner_state.authority.clone(),
                    inner_agg,
                )
                .unwrap(),
            );
            let batch_state = inner_state.authority.clone();
            tokio::task::spawn(async move {
                batch_state
                    .run_batch_service(1, Duration::from_secs(1))
                    .await
            });
            active_state
                .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
                .await;
        });
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.data().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Insert the certificates
    authority_state
        .enqueue_certificates_for_execution(certs.clone())
        .await
        .expect("Storage is ok");

    // Wait for execution.
    for cert in certs {
        wait_for_tx(*cert.digest(), authority_state.clone()).await;
    }
}

 */

async fn execute_owned_on_first_three_authorities(
    authority_clients: &[&SafeClient<LocalAuthorityClient>],
    committee: &Committee,
    txn: &VerifiedTransaction,
) -> (VerifiedCertificate, TransactionEffects) {
    do_transaction(authority_clients[0], txn).await;
    do_transaction(authority_clients[1], txn).await;
    do_transaction(authority_clients[2], txn).await;
    let cert = extract_cert(authority_clients, committee, txn.digest())
        .await
        .verify(committee)
        .unwrap();
    do_cert(authority_clients[0], &cert).await;
    do_cert(authority_clients[1], &cert).await;
    let effects = do_cert(authority_clients[2], &cert).await;
    (cert, effects)
}

pub async fn do_cert_with_shared_objects(
    authority: &AuthorityState,
    cert: &VerifiedCertificate,
) -> TransactionEffects {
    send_consensus(authority, cert).await;
    authority
        .database
        .notify_read_effects(vec![*cert.digest()])
        .await
        .unwrap()
        .pop()
        .unwrap()
        .into_data()
}

async fn execute_shared_on_first_three_authorities(
    authority_clients: &[&SafeClient<LocalAuthorityClient>],
    committee: &Committee,
    txn: &VerifiedTransaction,
) -> (VerifiedCertificate, TransactionEffects) {
    do_transaction(authority_clients[0], txn).await;
    do_transaction(authority_clients[1], txn).await;
    do_transaction(authority_clients[2], txn).await;
    let cert = extract_cert(authority_clients, committee, txn.digest())
        .await
        .verify(committee)
        .unwrap();
    do_cert_with_shared_objects(&authority_clients[0].authority_client().state, &cert).await;
    do_cert_with_shared_objects(&authority_clients[1].authority_client().state, &cert).await;
    let effects =
        do_cert_with_shared_objects(&authority_clients[2].authority_client().state, &cert).await;
    (cert, effects)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_transaction_manager() {
    telemetry_subscribers::init_for_testing();

    // ---- Initialize a network with three accounts, each with 10 gas objects.

    const NUM_ACCOUNTS: usize = 3;
    let accounts: Vec<(_, AccountKeyPair)> = (0..NUM_ACCOUNTS)
        .into_iter()
        .map(|_| get_key_pair())
        .collect_vec();

    const NUM_GAS_OBJECTS_PER_ACCOUNT: usize = 10;
    let gas_objects = (0..NUM_ACCOUNTS)
        .into_iter()
        .map(|i| {
            (0..NUM_GAS_OBJECTS_PER_ACCOUNT)
                .into_iter()
                .map(|_| Object::with_owner_for_testing(accounts[i].0))
                .collect_vec()
        })
        .collect_vec();
    let all_gas_objects = gas_objects.clone().into_iter().flatten().collect_vec();

    let (aggregator, authorities, framework_obj_ref) =
        init_local_authorities(4, all_gas_objects.clone()).await;
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| &aggregator.authority_clients[&a.name])
        .collect();

    // ---- Create an owned object and a shared counter.

    let mut executed_owned_certs = Vec::new();
    let mut executed_shared_certs = Vec::new();

    // Initialize an object owned by 1st account.
    let (addr1, key1): &(_, AccountKeyPair) = &accounts[0];
    let gas_ref = get_latest_ref(authority_clients[0], gas_objects[0][0].id()).await;
    let tx1 = create_object_move_transaction(*addr1, key1, *addr1, 100, framework_obj_ref, gas_ref);
    let (cert, effects1) =
        execute_owned_on_first_three_authorities(&authority_clients, &aggregator.committee, &tx1)
            .await;
    executed_owned_certs.push(cert);
    let mut owned_object_ref = effects1.created[0].0;

    // Initialize a shared counter, re-using gas_ref_0 so it has to execute after tx1.
    let gas_ref = get_latest_ref(authority_clients[0], gas_objects[0][0].id()).await;
    let tx2 = make_counter_create_transaction(gas_ref, framework_obj_ref, *addr1, key1);
    let (cert, effects2) =
        execute_owned_on_first_three_authorities(&authority_clients, &aggregator.committee, &tx2)
            .await;
    executed_owned_certs.push(cert);
    let (mut shared_counter_ref, owner) = effects2.created[0];
    let shared_counter_initial_version = if let Owner::Shared {
        initial_shared_version,
    } = owner
    {
        // Because the gas object used has version 2, the initial lamport timestamp of the shared
        // counter is 3.
        assert_eq!(initial_shared_version.value(), 3);
        initial_shared_version
    } else {
        panic!("Not a shared object! {:?} {:?}", shared_counter_ref, owner);
    };

    // ---- Execute transactions with dependencies on first 3 nodes in the dependency order.

    // In each iteration, creates an owned and a shared transaction that depends on previous input
    // and gas objects.
    for i in 0..100 {
        let source_index = i % NUM_ACCOUNTS;
        let (source_addr, source_key) = &accounts[source_index];

        let gas_ref = get_latest_ref(
            authority_clients[source_index],
            gas_objects[source_index][i * 3 % NUM_GAS_OBJECTS_PER_ACCOUNT].id(),
        )
        .await;
        let (dest_addr, _) = &accounts[(i + 1) % NUM_ACCOUNTS];
        let owned_tx = transfer_object_move_transaction(
            *source_addr,
            source_key,
            *dest_addr,
            owned_object_ref,
            framework_obj_ref,
            gas_ref,
        );
        let (cert, effects) = execute_owned_on_first_three_authorities(
            &authority_clients,
            &aggregator.committee,
            &owned_tx,
        )
        .await;
        executed_owned_certs.push(cert);
        owned_object_ref = effects.mutated_excluding_gas().next().unwrap().0;

        let gas_ref = get_latest_ref(
            authority_clients[source_index],
            gas_objects[source_index][i * 7 % NUM_GAS_OBJECTS_PER_ACCOUNT].id(),
        )
        .await;
        let shared_tx = make_counter_increment_transaction(
            gas_ref,
            framework_obj_ref,
            shared_counter_ref.0,
            shared_counter_initial_version,
            *source_addr,
            source_key,
        );
        let (cert, effects) = execute_shared_on_first_three_authorities(
            &authority_clients,
            &aggregator.committee,
            &shared_tx,
        )
        .await;
        executed_shared_certs.push(cert);
        shared_counter_ref = effects.mutated_excluding_gas().next().unwrap().0;
    }

    // ---- Execute transactions in reverse dependency order on the last authority.

    // Sets shared object locks in the executed order.
    for cert in executed_shared_certs.iter() {
        send_consensus_no_execution(&authorities[3], cert).await;
    }

    // Enqueue certs out of dependency order for executions.
    for cert in executed_shared_certs.iter().rev() {
        authorities[3]
            .enqueue_certificates_for_execution(
                vec![cert.clone()],
                &authorities[3].epoch_store_for_testing(),
            )
            .unwrap();
    }
    for cert in executed_owned_certs.iter().rev() {
        authorities[3]
            .enqueue_certificates_for_execution(
                vec![cert.clone()],
                &authorities[3].epoch_store_for_testing(),
            )
            .unwrap();
    }

    // All certs should get executed eventually.
    let digests = executed_shared_certs
        .iter()
        .chain(executed_owned_certs.iter())
        .map(|cert| *cert.digest())
        .collect();
    authorities[3]
        .database
        .notify_read_effects(digests)
        .await
        .unwrap();
}
