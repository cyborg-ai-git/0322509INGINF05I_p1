//! Regole bancarie e confronto con un oracolo intero indipendente.

use app_atm::{CAtm, CTransaction, ETransaction, EnumTransactionKind, EnumTransactionStatus};

/// Verifica il nuovo saldo e il rapporto del deposito completato.
#[test]
fn deposit_increases_the_balance_atomically() {
    let transaction = CTransaction::deposit(125).expect("valid deposit");

    let atm = CAtm::build("ATM3").expect("valid ATM");
    let (balance, outcome) = CTransaction::apply(&atm, 900, transaction.clone());

    assert_eq!(balance, 1025);
    assert_eq!(outcome.atm, atm);
    assert_eq!(outcome.transaction, transaction);
    assert_eq!(outcome.previous_balance, 900);
    assert_eq!(outcome.new_balance, 1025);
    assert_eq!(outcome.status, EnumTransactionStatus::Completed);
}

/// Verifica il prelievo ammissibile e i saldi prima e dopo.
#[test]
fn withdrawal_decreases_the_balance_when_funds_are_available() {
    let transaction = CTransaction::withdrawal(200).expect("valid withdrawal");

    let atm = CAtm::build("ATM2").expect("valid ATM");
    let (balance, outcome) = CTransaction::apply(&atm, 1000, transaction);

    assert_eq!(balance, 800);
    assert_eq!(outcome.status, EnumTransactionStatus::Completed);
    assert!(CTransaction::is_completed(&outcome));
}

/// Il rifiuto per fondi insufficienti lascia invariato il saldo e ne registra il motivo.
#[test]
fn withdrawal_is_rejected_when_funds_are_insufficient() {
    let transaction = CTransaction::withdrawal(501).expect("valid withdrawal");

    let atm = CAtm::build("ATM4").expect("valid ATM");
    let (balance, outcome) = CTransaction::apply(&atm, 500, transaction);

    assert_eq!(balance, 500);
    assert_eq!(outcome.new_balance, 500);
    assert!(matches!(
        outcome.status,
        EnumTransactionStatus::Rejected { reason } if reason.contains("fondi insufficienti")
    ));
}

/// Zero e importi negativi non sono richieste valide.
#[test]
fn non_positive_transactions_are_invalid() {
    assert!(CTransaction::deposit(0).is_err());
    assert!(CTransaction::withdrawal(-10).is_err());
}

/// Il parser riconosce le forme documentate e costruisce richieste di dominio.
#[test]
fn transaction_cli_parser_accepts_documented_forms() {
    let deposit: ETransaction = "deposit:100".parse().expect("deposit syntax");
    let withdrawal: ETransaction = "withdraw=75".parse().expect("withdraw syntax");

    assert_eq!(
        deposit,
        ETransaction {
            kind: EnumTransactionKind::Deposit,
            amount: 100
        }
    );
    assert_eq!(
        withdrawal,
        ETransaction {
            kind: EnumTransactionKind::Withdrawal,
            amount: 75
        }
    );
}

/// Anche i valori costruiti senza parser sono rivalidati prima dell’aritmetica.
#[test]
fn public_entity_values_are_revalidated_before_arithmetic() {
    let atm = CAtm::build("ATM1").unwrap();
    for amount in [i64::MIN, -1, 0] {
        for kind in [
            EnumTransactionKind::Deposit,
            EnumTransactionKind::Withdrawal,
        ] {
            let (balance, result) = CTransaction::apply(&atm, 1000, ETransaction { kind, amount });
            assert_eq!(balance, 1000);
            assert!(!CTransaction::is_completed(&result));
        }
    }
    let (balance, result) = CTransaction::apply(&atm, -1, CTransaction::deposit(10).unwrap());
    assert_eq!(balance, -1);
    assert!(!CTransaction::is_completed(&result));
}

/// Prova i confini di i64 senza arrotondamenti o ritorno circolare del contatore.
#[test]
fn money_boundaries_are_exact_and_overflow_is_rejected() {
    let atm = CAtm::build("ATM1").unwrap();
    let (balance, result) = CTransaction::apply(&atm, i64::MAX, CTransaction::deposit(1).unwrap());
    assert_eq!(balance, i64::MAX);
    assert!(!CTransaction::is_completed(&result));
    let (balance, result) =
        CTransaction::apply(&atm, i64::MAX, CTransaction::withdrawal(i64::MAX).unwrap());
    assert_eq!(balance, 0);
    assert!(CTransaction::is_completed(&result));
}

/// Rifiuta sintassi malformata, operazioni sconosciute e importi fuori intervallo.
#[test]
fn parser_rejects_unknown_malformed_and_out_of_range_requests() {
    for input in [
        "",
        "deposit",
        "transfer:20",
        "deposit:1.5",
        "withdraw:-1",
        "dep:0",
        "dep:9223372036854775808",
        "deposit:1:2",
    ] {
        assert!(input.parse::<ETransaction>().is_err(), "{input}");
    }
}

/// Confronta 22.220 combinazioni con un calcolo indipendente sul dominio ridotto.
#[test]
fn exhaustive_small_domain_matches_an_independent_integer_oracle() {
    let atm = CAtm::build("ATM1").unwrap();
    for balance in 0..=100 {
        for amount in 1..=110 {
            for kind in [
                EnumTransactionKind::Deposit,
                EnumTransactionKind::Withdrawal,
            ] {
                let (actual, result) =
                    CTransaction::apply(&atm, balance, ETransaction { kind, amount });
                let expected = match kind {
                    EnumTransactionKind::Deposit => balance + amount,
                    EnumTransactionKind::Withdrawal => {
                        if amount <= balance {
                            balance - amount
                        } else {
                            balance
                        }
                    }
                };
                assert_eq!(actual, expected);
                assert_eq!(result.previous_balance, balance);
                assert_eq!(result.new_balance, actual);
                assert!(actual >= 0);
            }
        }
    }
}
