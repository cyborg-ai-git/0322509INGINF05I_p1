#!/usr/bin/env bash
# Funzioni dei launcher per macOS e Linux, incluso Linux dentro WSL2.
# Compatibili con Bash 3.2 distribuito con macOS.
atm_error() {
  printf 'Errore: %s\n' "$*" >&2
  exit 1
}

atm_environment() {
  # Risolve la repository dallo script, indipendentemente dalla directory corrente.
  ATM_SCRIPTS="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
  ATM_ROOT="$(cd -- "$ATM_SCRIPTS/.." && pwd -P)"
  ATM_VERSION="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' "$ATM_ROOT/rust-toolchain.toml")"
  [[ "$ATM_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || atm_error 'Toolchain non valida in rust-toolchain.toml.'
  case "$(uname -s)" in
    Darwin) ATM_PLATFORM=macos ;;
    Linux) ATM_PLATFORM=linux ;;
    MINGW*|MSYS*|CYGWIN*) atm_error 'Su Windows usare Ubuntu dentro WSL2, non Git Bash o PowerShell.' ;;
    *) atm_error 'Sistemi supportati: macOS e Linux; su Windows usare WSL2.' ;;
  esac
  ATM_TOOLCHAIN="$ATM_VERSION"
  ATM_EXE="$ATM_ROOT/target/release/app_atm"
  ATM_ACCOUNT="${ATM_ACCOUNT:-$ATM_ROOT/shared.txt}"
  ATM_LOG_BASE="${ATM_LOG_BASE:-6001}"
  ATM_CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"
  export PATH="$ATM_CARGO_DIR/bin:$PATH"
}

atm_validate_numbers() {
  [[ "$1" =~ ^[0-9]+$ && ${#1} -le 5 ]] || atm_error 'La porta iniziale deve essere un intero.'
  [[ "$2" =~ ^[0-9]+$ && ${#2} -le 5 ]] || atm_error 'Il ritardo deve essere un intero in millisecondi.'
  ATM_BASE=$((10#$1))
  ATM_DELAY=$((10#$2))
  (( ATM_BASE >= 1024 && ATM_BASE <= 65532 )) || atm_error 'Porta iniziale ammessa: 1024-65532.'
  (( ATM_DELAY >= 0 && ATM_DELAY <= 60000 )) || atm_error 'Ritardo ammesso: 0-60000 ms.'
}

atm_command() {
  # Un solo bootstrap; i tre parametri di transazione riproducono lo scenario.
  local node="$1"
  [[ "$node" =~ ^[1-4]$ ]] || atm_error 'Nodo ammesso: 1, 2, 3 o 4.'
  ATM_COMMAND=("$ATM_EXE" --id "ATM$node" --bind "127.0.0.1:$((ATM_BASE+node-1))"
    --successor "127.0.0.1:$((ATM_BASE+node%4))" --transaction-delay-ms "$ATM_DELAY"
    --account-file "$ATM_ACCOUNT" --log-base-port "$ATM_LOG_BASE")
  case "$node" in
    1) ATM_COMMAND+=(--initial-token) ;;
    2) ATM_COMMAND+=(--transaction withdraw:200) ;;
    3) ATM_COMMAND+=(--transaction deposit:100) ;;
    4) ATM_COMMAND+=(--transaction withdraw:500) ;;
  esac
}
