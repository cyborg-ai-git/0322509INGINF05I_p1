//! Invarianti del token e FIFO locale verificati senza trasporto di rete.

use app_atm::{CAtm, CNode, CToken, CTransaction, EnumTransactionStatus, SPEC_INITIAL_BALANCE};

/// Confronta il successore di ogni ATM con la topologia prescritta.
#[test]
fn atm_successors_match_the_required_logical_ring() {
    let atm1 = CAtm::build("ATM1").expect("valid ATM");
    let atm2 = CAtm::build("ATM2").expect("valid ATM");
    let atm3 = CAtm::build("ATM3").expect("valid ATM");
    let atm4 = CAtm::build("ATM4").expect("valid ATM");

    assert_eq!(CAtm::successor(&atm1).unwrap(), atm2);
    assert_eq!(CAtm::successor(&atm2).unwrap(), atm3);
    assert_eq!(CAtm::successor(&atm3).unwrap(), atm4);
    assert_eq!(CAtm::successor(&atm4).unwrap(), atm1);
}

/// Controlla che gli alias accettati producano l’identificativo canonico.
#[test]
fn atm_entity_stores_its_identifier_as_a_string() {
    let atm = CAtm::build("atm2").expect("valid ATM");

    assert_eq!(atm.id, "ATM2");
}

/// Riproduce esattamente le visite richieste e il saldo finale 400.
#[test]
fn specification_example_produces_the_required_final_balance() {
    let atm1_entity = CAtm::build("ATM1").expect("valid ATM");
    let atm2_entity = CAtm::build("ATM2").expect("valid ATM");
    let atm3_entity = CAtm::build("ATM3").expect("valid ATM");
    let atm4_entity = CAtm::build("ATM4").expect("valid ATM");

    let mut atm1 = CNode::new(atm1_entity, []);
    let mut atm2 = CNode::new(
        atm2_entity,
        [CTransaction::withdrawal(200).expect("valid withdrawal")],
    );
    let mut atm3 = CNode::new(
        atm3_entity,
        [CTransaction::deposit(100).expect("valid deposit")],
    );
    let mut atm4 = CNode::new(
        atm4_entity.clone(),
        [CTransaction::withdrawal(500).expect("valid withdrawal")],
    );

    let report1 = atm1
        .process_token(CToken::create_initial(SPEC_INITIAL_BALANCE))
        .expect("valid token");
    assert_eq!(report1.forwarded_balance, 1000);
    assert!(report1.outcome.is_none());

    let report2 = atm2.process_token(report1.token).expect("valid token");
    assert_eq!(report2.forwarded_balance, 800);
    assert_eq!(
        report2.outcome.as_ref().expect("ATM2 transaction").status,
        EnumTransactionStatus::Completed
    );

    let report3 = atm3.process_token(report2.token).expect("valid token");
    assert_eq!(report3.forwarded_balance, 900);
    assert_eq!(
        report3.outcome.as_ref().expect("ATM3 transaction").status,
        EnumTransactionStatus::Completed
    );

    let report4 = atm4.process_token(report3.token).expect("valid token");
    assert_eq!(report4.forwarded_balance, 400);
    assert_eq!(report4.token.hop, 4);
    assert_eq!(report4.token.last_holder, Some(atm4_entity));
}

/// Una visita consuma una sola richiesta, lasciando le altre nella coda locale.
#[test]
fn only_one_queued_transaction_is_executed_per_token_pass() {
    let mut atm = CNode::new(
        CAtm::build("ATM2").expect("valid ATM"),
        [
            CTransaction::withdrawal(100).expect("valid withdrawal"),
            CTransaction::withdrawal(50).expect("valid withdrawal"),
        ],
    );

    let first = atm
        .process_token(CToken::create_initial(1000))
        .expect("valid token");
    assert_eq!(first.forwarded_balance, 900);
    assert_eq!(first.remaining_pending, 1);
    assert_eq!(atm.completed().len(), 1);

    let second = atm.process_token(first.token).expect("valid token");
    assert_eq!(second.forwarded_balance, 850);
    assert_eq!(second.remaining_pending, 0);
    assert_eq!(atm.completed().len(), 2);
}

/// Un nodo inattivo conserva il saldo e aggiorna soltanto i metadati della visita.
#[test]
fn token_without_pending_transaction_is_forwarded_unchanged_except_for_audit_fields() {
    let atm1 = CAtm::build("ATM1").expect("valid ATM");
    let mut atm = CNode::new(atm1.clone(), []);
    let report = atm
        .process_token(CToken::create_initial(777))
        .expect("valid token");

    assert_eq!(report.received_balance, 777);
    assert_eq!(report.forwarded_balance, 777);
    assert_eq!(report.token.hop, 1);
    assert_eq!(report.token.last_holder, Some(atm1));
    assert!(report.outcome.is_none());
}

/// Rifiuta duplicati, salti e predecessori errati rispetto alla visita attesa.
#[test]
fn route_validation_detects_duplicates_skipped_hops_and_wrong_holders() {
    let atm2 = CAtm::build("ATM2").unwrap();
    let mut token = CToken::create_initial(1000);
    CToken::mark_processed_by(&mut token, &CAtm::build("ATM1").unwrap()).unwrap();
    assert!(CToken::validate_route(&token, &atm2, 1).is_ok());
    assert!(CToken::validate_route(&token, &atm2, 5).is_err());
    token.last_holder = Some(CAtm::build("ATM4").unwrap());
    assert!(CToken::validate_route(&token, &atm2, 1).is_err());
    token.hop = 2;
    assert!(CToken::validate_route(&token, &atm2, 2).is_err());
}

/// Un token invalido non deve estrarre richieste né aggiornare lo storico.
#[test]
fn bad_token_cannot_consume_a_queued_transaction() {
    let mut node = CNode::new(
        CAtm::build("ATM1").unwrap(),
        [CTransaction::deposit(1).unwrap()],
    );
    let mut bad = CToken::create_initial(1000);
    bad.hop = u64::MAX;
    assert!(node.process_token(bad).is_err());
    assert!(node.process_token(CToken::create_initial(-1)).is_err());
    assert_eq!(node.pending_len(), 1);
    assert!(node.completed().is_empty());
}

/// Più giri conservano la FIFO locale e assegnano un turno a ogni nodo.
#[test]
fn multiple_rounds_preserve_fifo_and_give_every_node_a_turn() {
    let mut nodes: Vec<_> = (1..=4)
        .map(|i| {
            CNode::new(
                CAtm::build(i.to_string()).unwrap(),
                [
                    CTransaction::deposit(i).unwrap(),
                    CTransaction::withdrawal(i).unwrap(),
                ],
            )
        })
        .collect();
    let mut token = CToken::create_initial(1000);
    for hop in 0..40 {
        let node = &mut nodes[hop % 4];
        CToken::validate_route(&token, node.atm(), hop as u64).unwrap();
        token = node.process_token(token).unwrap().token;
    }
    assert_eq!(token.balance, 1000);
    assert_eq!(token.hop, 40);
    assert!(
        nodes
            .iter()
            .all(|n| n.pending_len() == 0 && n.completed().len() == 2)
    );
}

/// Un’identità costruita direttamente con campi invalidi produce un errore.
#[test]
fn public_invalid_identity_cannot_navigate_the_ring() {
    assert!(CAtm::successor(&app_atm::EAtm { id: "ATM5".into() }).is_err());
}
