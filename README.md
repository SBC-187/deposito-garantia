# Depósito de garantía de alquiler en Soroban 🏠

Contrato inteligente en Stellar (Soroban) que guarda el depósito de garantía de un alquiler para que **ni el inquilino ni el propietario puedan quedarse con el dinero sin el acuerdo del otro**.

> Tarea 1 — Campamento 02 (Contratos inteligentes en Soroban). Trabajo individual y práctico.

## El problema

En Costa Rica, cuando alquilás un apartamento, normalmente pagás un depósito de garantía (casi siempre un mes de alquiler). Ese dinero se le entrega directamente al propietario, y al terminar el contrato **devolverlo depende de su buena voluntad**:

- A veces tarda semanas o meses en devolverlo.
- A veces descuenta "daños" sin explicar cuánto ni por qué.
- A veces simplemente no responde.

El inquilino casi nunca tiene una forma rápida de reclamar. Y el propietario también necesita una garantía real de que, si hay daños, va a poder cubrirlos.

## La solución

El depósito no va a la cuenta del propietario: va al **contrato inteligente**, que funciona como una caja fuerte con reglas claras que nadie puede cambiar:

1. El propietario crea el acuerdo (inquilino, monto, token y fecha de fin del alquiler).
2. El inquilino deposita la garantía en el contrato.
3. Al terminar el alquiler, el propietario propone cuánto descontar por daños (puede ser 0).
4. El inquilino acepta y el contrato reparte el dinero automáticamente.
5. **Si el propietario no hace nada en 30 días** después del fin, el inquilino puede recuperar todo por su cuenta.

Nadie puede sacar el dinero solo: el propietario no puede descontar sin que el inquilino acepte, y no puede descontar más que el depósito ni antes de que termine el alquiler.

### ¿Por qué elegí este problema?

Es algo que le pasa a mucha gente que conozco, es pequeño y concreto, y encaja muy bien con lo que vimos en clase: **estado** (las etapas del depósito), **autorización** (quién puede firmar cada paso) y transferencias de tokens.

## Cómo funciona

### Estados

```
Creado ──depositar──▶ Depositado ──proponer_deduccion(0)──────────────▶ Cerrado
                          │                                                ▲
                          ├──proponer_deduccion(>0)──▶ DeduccionPropuesta ─┤ aceptar_deduccion
                          │                                                │
                          └──reclamar_por_vencimiento (30 días después) ───┘
```

### Funciones

| Función | Quién firma | Qué hace |
|---|---|---|
| `inicializar(propietario, inquilino, token, monto, fecha_fin)` | Propietario | Crea el acuerdo. Solo se puede llamar una vez. |
| `depositar()` | Inquilino | Mueve la garantía de la cuenta del inquilino al contrato. |
| `proponer_deduccion(deduccion)` | Propietario | Solo después de `fecha_fin`. Si es 0, devuelve todo al inquilino de una vez. |
| `aceptar_deduccion()` | Inquilino | Reparte: la deducción al propietario y el resto al inquilino. |
| `reclamar_por_vencimiento()` | Inquilino | Si pasaron 30 días desde `fecha_fin` sin propuesta, devuelve todo al inquilino. |
| `consultar()` | Nadie | Muestra el estado actual del acuerdo. |

El contrato también emite eventos (`deposito`, `propuesta`, `cerrado`) para que una app pueda mostrar el historial.

### Errores

| Código | Error | Cuándo pasa |
|---|---|---|
| 1 | `YaInicializado` | Se intenta crear el acuerdo dos veces |
| 2 | `NoInicializado` | Se llama una función antes de crear el acuerdo |
| 3 | `MontoInvalido` | Monto o deducción negativos / cero |
| 4 | `EstadoInvalido` | La acción no corresponde a la etapa actual |
| 5 | `AlquilerNoTermina` | El propietario intenta descontar antes de `fecha_fin` |
| 6 | `DeduccionExcedeDeposito` | Se intenta descontar más que el depósito |
| 7 | `PlazoNoVencido` | El inquilino reclama antes de los 30 días |
| 8 | `FechaInvalida` | La `fecha_fin` ya pasó al crear el acuerdo |

## Pruebas

Las pruebas están en [`contracts/deposito/src/test.rs`](contracts/deposito/src/test.rs) y usan un token de prueba (Stellar Asset Contract) y el reloj simulado del ledger.

| Prueba | Qué verifica |
|---|---|
| `deposito_mueve_el_dinero_al_contrato` | El dinero sale del inquilino y queda en el contrato |
| `depositar_exige_firma_del_inquilino` | Solo el inquilino puede autorizar el depósito |
| `devolucion_completa_si_no_hay_danos` | Deducción 0 → el inquilino recibe todo |
| `deduccion_aceptada_reparte_el_dinero` | 50 000 al propietario y 250 000 al inquilino |
| `no_se_puede_descontar_antes_del_fin_del_alquiler` | El propietario no puede adelantarse |
| `no_se_puede_descontar_mas_que_el_deposito` | La deducción tiene un tope |
| `inquilino_recupera_todo_si_el_propietario_no_responde` | Falla un segundo antes de los 30 días y funciona justo al cumplirse |
| `no_se_puede_depositar_dos_veces` | No se puede repetir una etapa |
| `no_se_puede_inicializar_dos_veces` | El acuerdo no se puede sobrescribir |
| `no_se_puede_aceptar_sin_propuesta` | No se puede aceptar algo que no existe |

Resultado:

```
test result: ok. 10 passed; 0 failed
```

## Cómo correrlo

### Requisitos

- [Rust](https://www.rust-lang.org/tools/install) con el target de WebAssembly: `rustup target add wasm32v1-none`
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli)

### Correr las pruebas

```bash
cargo test
```

### Compilar el contrato

```bash
stellar contract build
# genera target/wasm32v1-none/release/deposito_garantia.wasm
```

### Desplegar y probar en testnet

```bash
# 1. Crear y fondear dos cuentas de prueba
stellar keys generate propietario --network testnet --fund
stellar keys generate inquilino --network testnet --fund

# 2. Desplegar el contrato
stellar contract deploy \
  --wasm target/wasm32v1-none/release/deposito_garantia.wasm \
  --source propietario --network testnet --alias deposito

# 3. Usar XLM nativo como token
XLM=$(stellar contract id asset --asset native --network testnet)

# 4. Crear el acuerdo: 100 XLM (en stroops) que termina en 2 minutos
FIN=$(( $(date +%s) + 120 ))
stellar contract invoke --id deposito --source propietario --network testnet -- \
  inicializar --propietario propietario --inquilino inquilino \
  --token $XLM --monto 1000000000 --fecha_fin $FIN

# 5. El inquilino deposita
stellar contract invoke --id deposito --source inquilino --network testnet -- depositar

# 6. Ver el estado
stellar contract invoke --id deposito --source inquilino --network testnet -- consultar

# 7. Cuando pase la fecha de fin, el propietario propone descontar 10 XLM
stellar contract invoke --id deposito --source propietario --network testnet -- \
  proponer_deduccion --deduccion 100000000

# 8. El inquilino acepta y el dinero se reparte
stellar contract invoke --id deposito --source inquilino --network testnet -- aceptar_deduccion
```

## Estructura

```
.
├── Cargo.toml                     # workspace
├── contracts/deposito/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # el contrato
│       └── test.rs                # las pruebas
└── README.md
```

## Limitaciones y siguientes pasos

- **Desacuerdos:** si el inquilino no acepta la deducción, hoy el dinero queda en el contrato. El siguiente paso sería agregar un **árbitro** (por ejemplo, un tercero de confianza) que decida, como en un escrow por hitos.
- **Un acuerdo por contrato:** cada alquiler necesita su propio despliegue. Se podría guardar varios acuerdos con un ID.
- **Plazo fijo de 30 días:** podría ser un parámetro del acuerdo.

## Tecnologías

- Rust + [soroban-sdk 25](https://docs.rs/soroban-sdk)
- Stellar testnet
