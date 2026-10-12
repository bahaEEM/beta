#!/bin/bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Tek komutla oyna.
#   ./oyna.sh            -> sunucu + iki pencere bu bilgisayarda (tek başına deneme)
#   ./oyna.sh sunucu     -> sadece sunucu (arkadaşın kendi bilgisayarından bağlanır)
#   ./oyna.sh baglan IP  -> başka bilgisayardaki sunucuya bağlanan tek pencere (ör. ./oyna.sh baglan 192.168.1.20)
cd "$(dirname "$0")" || exit 1
. "$HOME/.cargo/env" 2>/dev/null
mode="${1:-ikisi}"
start_server() {
  cargo build -q --release -p server || exit 1
  target/release/server > saves/sunucu.log 2>&1 &
  echo "Sunucu açıldı (PID $!). Kayıt: saves/sunucu.log"
  sleep 2
  grep -a "LAN players connect to" saves/sunucu.log | sed 's/.*connect to: /Arkadaşınızın gireceği adres: /'
}
case "$mode" in
  sunucu)
    start_server
    echo "Kapatmak için: kill $(pgrep -n -f target/release/server)"
    wait ;;
  baglan)
    godot --path client -- --name "${3:-oyuncu2}" &
    echo "Pencerede adres kutusuna ${2:-SUNUCU_IP}:7777 yazıp Bağlan'a basın." ;;
  *)
    start_server
    godot --path client --position 0,30 --resolution 955x1000 -- --name mavi --connect &
    sleep 2
    godot --path client --position 965,30 --resolution 955x1000 -- --name kirmizi --connect &
    echo "İki pencere açıldı: solda Mavi (kuzey), sağda Kırmızı (güney)." ;;
esac
