#!/usr/bin/env bash
# Compila e apre quattro console indipendenti; il timer limita solo la dimostrazione.
set -euo pipefail
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/run_common.sh"

# I marcatori appartengono alla sola esecuzione corrente: non coordinano il token.
# Il primo motivo di arresto prevale, anche quando due console terminano insieme.
atm_request_stop() {
  (set -o noclobber; printf '%s\n' "$1" > "$ATM_RUN_DIR/stop") 2>/dev/null || true
}

atm_all_marked() {
  local n
  for n in 1 2 3 4; do
    [[ -f "$ATM_RUN_DIR/$1.$n" ]] || return 1
  done
}

atm_validate_duration() {
  [[ "$1" =~ ^[0-9]+$ && ${#1} -le 4 ]] || atm_error 'La durata deve essere un intero in secondi.'
  ATM_DURATION=$((10#$1))
  (( ATM_DURATION >= 1 && ATM_DURATION <= 3600 )) || atm_error 'Durata ammessa: 1-3600 secondi.'
}

# Modalità interna eseguita dentro ciascuna finestra. La shell conserva il PID
# del proprio figlio: non cerca processi per nome e non arresta altri elaborati.
atm_timed_node() {
  [[ $# -eq 6 && "$1" =~ ^[1-4]$ ]] || atm_error 'Parametri interni della console non validi.'
  local node="$1" base_port="$2" node_delay="$3" account="$4" udp_base="$5"
  local child='' started="$SECONDS" clock_started=false reason attempt
  atm_validate_duration "$6"
  ATM_ACCOUNT="$account"
  atm_environment
  ATM_RUN_DIR="$(dirname -- "$account")"
  [[ -d "$ATM_RUN_DIR" ]] || atm_error 'Cartella della dimostrazione assente.'

  atm_node_cleanup() {
    # Sospende prima i quattro figli, poi li termina. Questo evita che un ATM
    # ancora attivo invii sul collegamento appena chiuso da un altro al termine.
    atm_request_stop errore
    if [[ -n "$child" ]]; then
      kill -STOP "$child" 2>/dev/null || true
      : > "$ATM_RUN_DIR/stopped.$node"
      for attempt in {1..20}; do
        atm_all_marked stopped && break
        sleep 0.1
      done
      kill -TERM "$child" 2>/dev/null || true
      kill -CONT "$child" 2>/dev/null || true
      for attempt in {1..20}; do
        kill -0 "$child" 2>/dev/null || break
        sleep 0.1
      done
      if kill -0 "$child" 2>/dev/null; then kill -KILL "$child" 2>/dev/null || true; fi
      wait "$child" 2>/dev/null || true
      child=''
    fi
    : > "$ATM_RUN_DIR/finished.$node"
  }
  trap atm_node_cleanup EXIT
  trap 'atm_request_stop interrotto; exit 130' INT
  trap 'atm_request_stop interrotto; exit 143' TERM HUP
  bash "$ATM_SCRIPTS/run_node.sh" "$node" "$base_port" "$node_delay" "$account" "$udp_base" &
  child=$!
  : > "$ATM_RUN_DIR/ready.$node"

  while [[ ! -f "$ATM_RUN_DIR/stop" ]]; do
    if ! kill -0 "$child" 2>/dev/null; then
      atm_request_stop errore
      break
    fi
    if [[ -f "$ATM_RUN_DIR/start" ]]; then
      if ! $clock_started; then started="$SECONDS"; clock_started=true; fi
      # Protezione anche se la finestra del launcher viene chiusa forzatamente.
      if (( SECONDS - started >= ATM_DURATION + 5 )); then atm_request_stop errore; fi
    elif (( SECONDS - started >= 30 )); then
      atm_request_stop errore
    fi
    sleep 0.1
  done
  atm_node_cleanup
  trap - EXIT INT TERM HUP
  reason="$(cat "$ATM_RUN_DIR/stop")"
  if [[ "$reason" == timer ]]; then
    printf '\nATM%s: dimostrazione terminata. Timer di %s secondi scaduto; processo arrestato.\n' "$node" "$ATM_DURATION"
  else
    printf '\nATM%s: dimostrazione interrotta; processo arrestato.\n' "$node"
  fi
  # Il processo Rust è già terminato. La sola attesa di Invio mantiene leggibile
  # il risultato nei terminali che chiudono la finestra alla fine del comando.
  if [[ -t 0 ]]; then read -r -p 'Premi Invio per uscire dalla sessione. ' || true; fi
  [[ "$reason" == timer ]]
}

if [[ "${1:-}" == --timed-node ]]; then
  shift
  atm_timed_node "$@"
  exit $?
fi

base=7001
log_base=6001
delay=1500
duration=20
terminal=auto
dry_run=false
build=true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help)
      printf '%s\n' 'Uso: bash scripts/run_elaborato.sh [opzioni]' \
        '--base-port N     Prima di quattro porte libere consecutive (default 7001)' \
        '--log-base-port N Prima di quattro porte UDP dei log (default 6001)' \
        '--delay-ms N      Ritardo della sola transazione (default 1500)' \
        '--duration N      Arresto automatico dopo N secondi (default 20; non conta i giri)' \
        '--terminal NOME   Linux: auto, gnome-terminal, konsole, xfce4-terminal, xterm' \
        '--no-build        Usa il binario release gia compilato' \
        '--dry-run         Stampa i quattro comandi senza compilare o aprire finestre'
      exit 0 ;;
    --base-port|--log-base-port|--delay-ms|--duration|--terminal)
      [[ $# -ge 2 ]] || atm_error "Manca il valore per $1."
      case "$1" in
        --base-port) base="$2" ;;
        --log-base-port) log_base="$2" ;;
        --delay-ms) delay="$2" ;;
        --duration) duration="$2" ;;
        --terminal) terminal="$2" ;;
      esac
      shift 2 ;;
    --no-build) build=false; shift ;;
    --dry-run) dry_run=true; shift ;;
    *) atm_error "Opzione sconosciuta: $1" ;;
  esac
done
atm_environment
atm_validate_duration "$duration"
atm_validate_numbers "$log_base" 0
ATM_LOG_BASE="$ATM_BASE"
atm_validate_numbers "$base" "$delay"
ATM_ACCOUNT="$ATM_ROOT/.local/runs/NUOVA_ESECUZIONE/shared.txt"
case "$terminal" in auto|gnome-terminal|konsole|xfce4-terminal|xterm) ;; *) atm_error 'Emulatore di terminale non supportato.' ;; esac

if $dry_run; then
  printf 'Sistema: %s | toolchain: %s\n' "$ATM_PLATFORM" "$ATM_TOOLCHAIN"
  printf 'Durata della dimostrazione: %s secondi; arresto automatico dei quattro ATM.\n' "$ATM_DURATION"
  for node in 1 2 3 4; do
    atm_command "$node"
    printf '%q ' "${ATM_COMMAND[@]}"
    printf '\n'
  done
  exit 0
fi

# Controlla il desktop prima di compilare.
case "$ATM_PLATFORM" in
  macos) command -v osascript >/dev/null || atm_error 'AppleScript non disponibile.' ;;
  linux)
    [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]] || atm_error 'Serve un desktop Linux; in WSL2 verificare WSLg, oppure usare quattro sessioni Ubuntu manuali.'
    if [[ "$terminal" == auto ]]; then
      for candidate in gnome-terminal konsole xfce4-terminal xterm; do
        if command -v "$candidate" >/dev/null; then terminal="$candidate"; break; fi
      done
    fi
    [[ "$terminal" != auto ]] && command -v "$terminal" >/dev/null || \
      atm_error 'Installare un terminale: gnome-terminal, konsole, xfce4-terminal oppure xterm.'
    ;;
esac
if $build; then
  command -v rustup >/dev/null || atm_error 'Eseguire prima bash scripts/run_install.sh.'
  (cd -- "$ATM_ROOT" && rustup run "$ATM_TOOLCHAIN" cargo build --release --target-dir "$ATM_ROOT/target")
fi
[[ -f "$ATM_EXE" ]] || atm_error 'Binario release assente.'


# Ogni avvio usa un conto nuovo; tutti i nodi ricevono lo stesso percorso assoluto.
mkdir -p -- "$ATM_ROOT/.local/runs"
ATM_RUN_DIR="$(mktemp -d "$ATM_ROOT/.local/runs/run_XXXXXX")"
ATM_ACCOUNT="$ATM_RUN_DIR/shared.txt"
trap 'atm_request_stop interrotto' EXIT
trap 'atm_request_stop interrotto; exit 130' INT
trap 'atm_request_stop interrotto; exit 143' TERM HUP

if [[ "$ATM_PLATFORM" == macos ]]; then
  commands=()
  for node in 1 2 3 4; do
    printf -v command '%q ' /bin/bash "$ATM_SCRIPTS/run_elaborato.sh" --timed-node "$node" "$ATM_BASE" "$ATM_DELAY" "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$ATM_DURATION"
    commands+=("$command")
  done
  osascript "$ATM_SCRIPTS/run_macos.applescript" "${commands[@]}"
else
  # Ogni invocazione richiede una finestra nuova e un comando distinto.
  for node in 1 2 3 4; do
    case "$terminal" in
      gnome-terminal)
        gnome-terminal --window --title="ATM$node" -- bash "$ATM_SCRIPTS/run_elaborato.sh" --timed-node "$node" "$ATM_BASE" "$ATM_DELAY" "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$ATM_DURATION" ;;
      konsole)
        konsole --separate -p "tabtitle=ATM$node" -e bash "$ATM_SCRIPTS/run_elaborato.sh" --timed-node "$node" "$ATM_BASE" "$ATM_DELAY" "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$ATM_DURATION" & ;;
      xfce4-terminal)
        printf -v command '%q ' bash "$ATM_SCRIPTS/run_elaborato.sh" --timed-node "$node" "$ATM_BASE" "$ATM_DELAY" "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$ATM_DURATION"
        xfce4-terminal --disable-server --title="ATM$node" --command="$command" & ;;
      xterm)
        xterm -T "ATM$node" -geometry 100x28 -e bash "$ATM_SCRIPTS/run_elaborato.sh" --timed-node "$node" "$ATM_BASE" "$ATM_DELAY" "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$ATM_DURATION" & ;;
    esac
  done
fi
printf 'Avvio richiesto per quattro terminali: ATM1-ATM4, porte %s-%s.\n' "$ATM_BASE" "$((ATM_BASE+3))"
printf 'File del conto: %s | porte UDP: %s-%s\n' "$ATM_ACCOUNT" "$ATM_LOG_BASE" "$((ATM_LOG_BASE+3))"
printf '%s\n' 'Attendo l’avvio delle quattro console. Ctrl+C interrompe questa dimostrazione.'

# Il timer parte solo quando tutte le console hanno avviato il proprio figlio.
startup="$SECONDS"
while ! atm_all_marked ready; do
  [[ ! -f "$ATM_RUN_DIR/stop" ]] || break
  if (( SECONDS - startup >= 30 )); then atm_request_stop errore; break; fi
  sleep 0.1
done
if [[ ! -f "$ATM_RUN_DIR/stop" ]]; then
  : > "$ATM_RUN_DIR/start"
  remaining="$ATM_DURATION"
  printf 'Dimostrazione avviata: arresto automatico fra %s secondi.\n' "$ATM_DURATION"
  while (( remaining > 0 )); do
    [[ ! -f "$ATM_RUN_DIR/stop" ]] || break
    sleep 1
    remaining=$((remaining-1))
  done
  atm_request_stop timer
fi
for attempt in {1..60}; do
  atm_all_marked finished && break
  sleep 0.1
done
trap - EXIT INT TERM HUP
if ! atm_all_marked finished; then
  atm_error 'Avvio o arresto incompleto delle console; controllare le finestre aperte.'
fi
[[ "$(cat "$ATM_RUN_DIR/stop")" == timer ]] || atm_error 'Dimostrazione interrotta: un nodo o una console è terminato prima del timer.'
balance="$(cat "$ATM_ACCOUNT" 2>/dev/null || true)"
printf '\nDimostrazione terminata: timer di %s secondi scaduto, quattro ATM arrestati.\n' "$ATM_DURATION"
printf 'Saldo raggiunto: %s | File del conto: %s\n' "${balance:-non disponibile}" "$ATM_ACCOUNT"
[[ "$balance" == 400 ]] || atm_error 'Scenario non completato: aumentare --duration o controllare i log delle console.'
printf '%s\n' 'Scenario completato: saldo finale 400. Le console restano disponibili per leggere il risultato.'
