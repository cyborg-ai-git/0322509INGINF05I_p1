//! Configurazioni valide e rifiuto dei parametri incompatibili con i quattro nodi.

use app_atm::{CAtm, CConfig};
use std::net::SocketAddr;
fn address(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
}

/// Verifica tutte le quattro configurazioni e la chiusura ATM4 → ATM1.
#[test]
fn all_four_nodes_have_exactly_the_required_successor() {
    for base in [1, 7001, 65532] {
        for i in 0..4 {
            let atm = CAtm::build(format!("ATM{}", i + 1)).unwrap();
            assert!(
                CConfig::validate(
                    &atm,
                    address(base + i),
                    address(base + (i + 1) % 4),
                    i == 0,
                    1000
                )
                .is_ok()
            );
        }
    }
}

/// Rifiuta topologie, bootstrap e saldi incompatibili prima dell’avvio del listener.
#[test]
fn invalid_topologies_initializers_and_balances_are_rejected() {
    let atm1 = CAtm::build("ATM1").unwrap();
    let atm2 = CAtm::build("ATM2").unwrap();
    assert!(CConfig::validate(&atm1, address(7001), address(7001), true, 1000).is_err());
    assert!(CConfig::validate(&atm1, address(7001), address(7002), false, 1000).is_err());
    assert!(CConfig::validate(&atm2, address(7002), address(7003), true, 1000).is_err());
    assert!(CConfig::validate(&atm1, address(7001), address(7002), true, -1).is_err());
    assert!(CConfig::validate(&atm1, address(65535), address(13000), true, 1000).is_ok());
    assert!(CConfig::validate(&atm1, address(1), address(0), true, 1000).is_err());
    assert!(CConfig::validate(&atm1, address(0), address(1), true, 1000).is_err());
    assert!(
        CConfig::validate(
            &atm1,
            "0.0.0.0:7001".parse().unwrap(),
            address(7002),
            true,
            1000
        )
        .is_err()
    );
    assert!(
        CConfig::validate(
            &atm1,
            address(7001),
            "192.0.2.1:7002".parse().unwrap(),
            true,
            1000
        )
        .is_err()
    );
    for id in ["ATM0", "ATM5", "", "garbage"] {
        assert!(CAtm::build(id).is_err());
    }
}

/// Il parametro numerico sceglie il singolo nodo e il successore si calcola modulo quattro.
#[test]
fn numeric_parameters_and_independent_ports_preserve_the_logical_ring() {
    let ports = [32001, 17003, 49111, 8008];
    for number in 1..=CAtm::NODE_COUNT {
        let atm = CAtm::build(number.to_string()).unwrap();
        assert_eq!(atm.id, format!("ATM{number}"));
        assert_eq!(CAtm::index(&atm).unwrap(), number - 1);
        assert_eq!(
            CAtm::successor(&atm).unwrap().id,
            format!("ATM{}", number % 4 + 1)
        );
        assert!(
            CConfig::validate(
                &atm,
                address(ports[(number - 1) as usize]),
                address(ports[(number % 4) as usize]),
                number == 1,
                1000
            )
            .is_ok()
        );
    }
    for invalid in ["0", "5", "-1", "+1", "01", "ATM01", "65536"] {
        assert!(CAtm::build(invalid).is_err());
    }
}
