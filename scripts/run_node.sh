#!/usr/bin/env bash
# Eseguito dentro una sola finestra: questa shell viene sostituita da un solo ATM.
set -euo pipefail
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/run_common.sh"
[[ $# -eq 5 ]] || atm_error 'Uso interno: run_node.sh NODO PORTA_BASE RITARDO_MS FILE_CONTO PORTA_LOG_BASE.'
ATM_ACCOUNT="$4"
ATM_LOG_BASE="$5"
atm_environment
atm_validate_numbers "$2" "$3"
atm_command "$1"
[[ -x "$ATM_EXE" ]] || atm_error 'Binario assente: avviare prima run_elaborato.sh senza --no-build.'
cd -- "$ATM_ROOT"
printf '\033]0;ATM%s - Token Ring\007' "$1"
printf 'ATM%s | PID %s | Ctrl+C interrompe questo nodo\n' "$1" "$$"
exec "${ATM_COMMAND[@]}"
