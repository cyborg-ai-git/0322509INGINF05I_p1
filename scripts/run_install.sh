#!/usr/bin/env bash
# Installa Rust su macOS/Linux, anche dentro WSL2, e verifica il linker reale.
set -euo pipefail
source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/run_common.sh"
[[ ${1:-} != --help ]] || {
  printf '%s\n' 'Uso: bash scripts/run_install.sh' 'Su Windows aprire Ubuntu in WSL2 e usare gli stessi comandi Linux.'
  exit 0
}
[[ $# -eq 0 ]] || atm_error 'Argomento non riconosciuto; usare --help.'
atm_environment
ATM_INSTALL_TMP="$(mktemp -d)"
trap 'rm -rf -- "$ATM_INSTALL_TMP"' EXIT
if ! command -v rustup >/dev/null 2>&1; then
  command -v curl >/dev/null || atm_error 'Installare curl prima di eseguire lo script.'
  installer="$ATM_INSTALL_TMP/rustup-init.sh"
  curl --proto '=https' --tlsv1.2 -fLsS --retry 3 https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --profile minimal --default-toolchain none
fi
rustup toolchain install "$ATM_TOOLCHAIN" --profile minimal --component rustfmt,clippy
# Compila un programma minimo: trovare Cargo non prova che SDK e linker funzionino.
printf 'fn main() {}\n' > "$ATM_INSTALL_TMP/linker_probe.rs"
if ! rustup run "$ATM_TOOLCHAIN" rustc --crate-name atm_linker_probe \
  "$ATM_INSTALL_TMP/linker_probe.rs" -o "$ATM_INSTALL_TMP/linker_probe"; then
  case "$ATM_PLATFORM" in
    macos) atm_error 'Installare le Xcode Command Line Tools con xcode-select --install, poi riprovare.' ;;
    linux) atm_error 'Installare il linker C: build-essential su Debian/Ubuntu, gcc e strumenti di sviluppo sulle altre distribuzioni.' ;;
  esac
fi
rustup run "$ATM_TOOLCHAIN" rustc --version
rustup run "$ATM_TOOLCHAIN" cargo --version
printf 'Ambiente verificato: %s, toolchain %s.\n' "$ATM_PLATFORM" "$ATM_TOOLCHAIN"
printf '%s\n' 'Per avviare i quattro terminali: bash scripts/run_elaborato.sh'
