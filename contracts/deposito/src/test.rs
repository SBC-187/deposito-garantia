#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Symbol,
};

const MONTO: i128 = 300_000; // p. ej. ₡300 000 representados en el token
const FIN: u64 = 1_000_000;

struct Escenario<'a> {
    env: Env,
    cliente: DepositoGarantiaClient<'a>,
    contrato: Address,
    token: TokenClient<'a>,
    propietario: Address,
    inquilino: Address,
}

fn preparar() -> Escenario<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let admin = Address::generate(&env);
    let propietario = Address::generate(&env);
    let inquilino = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin);
    let token_id = sac.address();
    StellarAssetClient::new(&env, &token_id).mint(&inquilino, &MONTO);

    let contrato = env.register(DepositoGarantia, ());
    let cliente = DepositoGarantiaClient::new(&env, &contrato);
    cliente.inicializar(&propietario, &inquilino, &token_id, &MONTO, &FIN);

    Escenario {
        token: TokenClient::new(&env, &token_id),
        env,
        cliente,
        contrato,
        propietario,
        inquilino,
    }
}

fn ir_a(env: &Env, ts: u64) {
    env.ledger().with_mut(|l| l.timestamp = ts);
}

#[test]
fn deposito_mueve_el_dinero_al_contrato() {
    let e = preparar();
    e.cliente.depositar();

    assert_eq!(e.token.balance(&e.inquilino), 0);
    assert_eq!(e.token.balance(&e.contrato), MONTO);
    assert_eq!(e.cliente.consultar().estado, Estado::Depositado);
}

#[test]
fn depositar_exige_firma_del_inquilino() {
    let e = preparar();
    e.cliente.depositar();

    let auths = e.env.auths();
    assert_eq!(auths[0].0, e.inquilino);
    match &auths[0].1.function {
        AuthorizedFunction::Contract((id, nombre, _)) => {
            assert_eq!(id, &e.contrato);
            assert_eq!(nombre, &Symbol::new(&e.env, "depositar"));
        }
        _ => panic!("se esperaba una autorización de contrato"),
    }
}

#[test]
fn devolucion_completa_si_no_hay_danos() {
    let e = preparar();
    e.cliente.depositar();
    ir_a(&e.env, FIN);

    e.cliente.proponer_deduccion(&0);

    assert_eq!(e.token.balance(&e.inquilino), MONTO);
    assert_eq!(e.token.balance(&e.contrato), 0);
    assert_eq!(e.cliente.consultar().estado, Estado::Cerrado);
}

#[test]
fn deduccion_aceptada_reparte_el_dinero() {
    let e = preparar();
    e.cliente.depositar();
    ir_a(&e.env, FIN + 10);

    e.cliente.proponer_deduccion(&50_000);
    assert_eq!(e.cliente.consultar().estado, Estado::DeduccionPropuesta);
    // El dinero sigue en el contrato hasta que el inquilino acepte.
    assert_eq!(e.token.balance(&e.contrato), MONTO);

    e.cliente.aceptar_deduccion();
    assert_eq!(e.token.balance(&e.propietario), 50_000);
    assert_eq!(e.token.balance(&e.inquilino), 250_000);
    assert_eq!(e.cliente.consultar().estado, Estado::Cerrado);
}

#[test]
fn no_se_puede_descontar_antes_del_fin_del_alquiler() {
    let e = preparar();
    e.cliente.depositar();

    let r = e.cliente.try_proponer_deduccion(&10_000);
    assert_eq!(r, Err(Ok(Error::AlquilerNoTermina)));
}

#[test]
fn no_se_puede_descontar_mas_que_el_deposito() {
    let e = preparar();
    e.cliente.depositar();
    ir_a(&e.env, FIN);

    let r = e.cliente.try_proponer_deduccion(&(MONTO + 1));
    assert_eq!(r, Err(Ok(Error::DeduccionExcedeDeposito)));
}

#[test]
fn inquilino_recupera_todo_si_el_propietario_no_responde() {
    let e = preparar();
    e.cliente.depositar();

    ir_a(&e.env, FIN + PLAZO_RECLAMO_SEGUNDOS - 1);
    assert_eq!(
        e.cliente.try_reclamar_por_vencimiento(),
        Err(Ok(Error::PlazoNoVencido))
    );

    ir_a(&e.env, FIN + PLAZO_RECLAMO_SEGUNDOS);
    e.cliente.reclamar_por_vencimiento();
    assert_eq!(e.token.balance(&e.inquilino), MONTO);
    assert_eq!(e.cliente.consultar().estado, Estado::Cerrado);
}

#[test]
fn no_se_puede_depositar_dos_veces() {
    let e = preparar();
    StellarAssetClient::new(&e.env, &e.token.address).mint(&e.inquilino, &MONTO);
    e.cliente.depositar();

    assert_eq!(e.cliente.try_depositar(), Err(Ok(Error::EstadoInvalido)));
}

#[test]
fn no_se_puede_inicializar_dos_veces() {
    let e = preparar();
    let r = e
        .cliente
        .try_inicializar(&e.propietario, &e.inquilino, &e.token.address, &MONTO, &FIN);
    assert_eq!(r, Err(Ok(Error::YaInicializado)));
}

#[test]
fn no_se_puede_aceptar_sin_propuesta() {
    let e = preparar();
    e.cliente.depositar();
    assert_eq!(
        e.cliente.try_aceptar_deduccion(),
        Err(Ok(Error::EstadoInvalido))
    );
}
