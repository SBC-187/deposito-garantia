#![no_std]
//! Depósito de garantía de alquiler en Soroban.
//!
//! El inquilino deposita la garantía en el contrato (no en la cuenta del
//! propietario). Al terminar el alquiler, el propietario propone cuánto
//! descontar por daños (puede ser 0) y el inquilino lo acepta. Si el
//! propietario no hace nada en 30 días después del fin del contrato, el
//! inquilino puede recuperar el depósito completo por su cuenta.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
};

/// Días que tiene el propietario, después del fin del alquiler, para
/// proponer una deducción antes de que el inquilino pueda reclamar todo.
pub const PLAZO_RECLAMO_SEGUNDOS: u64 = 30 * 24 * 60 * 60;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    YaInicializado = 1,
    NoInicializado = 2,
    MontoInvalido = 3,
    EstadoInvalido = 4,
    AlquilerNoTermina = 5,
    DeduccionExcedeDeposito = 6,
    PlazoNoVencido = 7,
    FechaInvalida = 8,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Estado {
    /// Contrato creado, esperando que el inquilino deposite.
    Creado,
    /// El dinero está en el contrato durante el alquiler.
    Depositado,
    /// El propietario propuso una deducción; falta que el inquilino acepte.
    DeduccionPropuesta,
    /// El dinero ya se repartió. No se puede hacer nada más.
    Cerrado,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Garantia {
    pub inquilino: Address,
    pub propietario: Address,
    pub token: Address,
    pub monto: i128,
    /// Fecha de fin del alquiler (timestamp Unix en segundos).
    pub fecha_fin: u64,
    pub deduccion: i128,
    pub estado: Estado,
}

#[contracttype]
enum DataKey {
    Garantia,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deposito {
    pub monto: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Propuesta {
    pub deduccion: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cerrado {
    pub al_propietario: i128,
    pub al_inquilino: i128,
}

#[contract]
pub struct DepositoGarantia;

fn leer(env: &Env) -> Result<Garantia, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Garantia)
        .ok_or(Error::NoInicializado)
}

fn guardar(env: &Env, g: &Garantia) {
    env.storage().instance().set(&DataKey::Garantia, g);
}

/// Reparte el dinero del contrato y lo deja cerrado.
fn liquidar(env: &Env, g: &mut Garantia) {
    let cliente = token::Client::new(env, &g.token);
    let contrato = env.current_contract_address();
    let al_inquilino = g.monto - g.deduccion;

    if g.deduccion > 0 {
        cliente.transfer(&contrato, &g.propietario, &g.deduccion);
    }
    if al_inquilino > 0 {
        cliente.transfer(&contrato, &g.inquilino, &al_inquilino);
    }
    g.estado = Estado::Cerrado;
    guardar(env, g);
    Cerrado {
        al_propietario: g.deduccion,
        al_inquilino,
    }
    .publish(env);
}

#[contractimpl]
impl DepositoGarantia {
    /// El propietario crea el acuerdo: quién es el inquilino, en qué token,
    /// cuánto es la garantía y cuándo termina el alquiler.
    pub fn inicializar(
        env: Env,
        propietario: Address,
        inquilino: Address,
        token: Address,
        monto: i128,
        fecha_fin: u64,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Garantia) {
            return Err(Error::YaInicializado);
        }
        propietario.require_auth();
        if monto <= 0 {
            return Err(Error::MontoInvalido);
        }
        if fecha_fin <= env.ledger().timestamp() {
            return Err(Error::FechaInvalida);
        }
        guardar(
            &env,
            &Garantia {
                inquilino,
                propietario,
                token,
                monto,
                fecha_fin,
                deduccion: 0,
                estado: Estado::Creado,
            },
        );
        Ok(())
    }

    /// El inquilino envía la garantía al contrato.
    pub fn depositar(env: Env) -> Result<(), Error> {
        let mut g = leer(&env)?;
        g.inquilino.require_auth();
        if g.estado != Estado::Creado {
            return Err(Error::EstadoInvalido);
        }
        token::Client::new(&env, &g.token).transfer(
            &g.inquilino,
            env.current_contract_address(),
            &g.monto,
        );
        g.estado = Estado::Depositado;
        guardar(&env, &g);
        Deposito { monto: g.monto }.publish(&env);
        Ok(())
    }

    /// Al terminar el alquiler, el propietario dice cuánto quiere descontar.
    /// Si propone 0, el inquilino recibe todo de inmediato.
    pub fn proponer_deduccion(env: Env, deduccion: i128) -> Result<(), Error> {
        let mut g = leer(&env)?;
        g.propietario.require_auth();
        if g.estado != Estado::Depositado {
            return Err(Error::EstadoInvalido);
        }
        if env.ledger().timestamp() < g.fecha_fin {
            return Err(Error::AlquilerNoTermina);
        }
        if deduccion < 0 {
            return Err(Error::MontoInvalido);
        }
        if deduccion > g.monto {
            return Err(Error::DeduccionExcedeDeposito);
        }
        g.deduccion = deduccion;
        if deduccion == 0 {
            liquidar(&env, &mut g);
        } else {
            g.estado = Estado::DeduccionPropuesta;
            guardar(&env, &g);
            Propuesta { deduccion }.publish(&env);
        }
        Ok(())
    }

    /// El inquilino acepta la deducción y el dinero se reparte.
    pub fn aceptar_deduccion(env: Env) -> Result<(), Error> {
        let mut g = leer(&env)?;
        g.inquilino.require_auth();
        if g.estado != Estado::DeduccionPropuesta {
            return Err(Error::EstadoInvalido);
        }
        liquidar(&env, &mut g);
        Ok(())
    }

    /// Si el propietario no propone nada en 30 días después del fin del
    /// alquiler, el inquilino recupera la garantía completa.
    pub fn reclamar_por_vencimiento(env: Env) -> Result<(), Error> {
        let mut g = leer(&env)?;
        g.inquilino.require_auth();
        if g.estado != Estado::Depositado {
            return Err(Error::EstadoInvalido);
        }
        if env.ledger().timestamp() < g.fecha_fin + PLAZO_RECLAMO_SEGUNDOS {
            return Err(Error::PlazoNoVencido);
        }
        g.deduccion = 0;
        liquidar(&env, &mut g);
        Ok(())
    }

    /// Cualquiera puede consultar el estado del acuerdo.
    pub fn consultar(env: Env) -> Result<Garantia, Error> {
        leer(&env)
    }
}

#[cfg(test)]
mod test;
