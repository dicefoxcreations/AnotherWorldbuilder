#!/usr/bin/env bash
# Joins a world between two network stacks that both sit behind a firewall
# like ufw's default: nothing starting outside comes in, only mDNS and the
# answers to what went out. Needs no root — everything runs in namespaces of
# the calling user and disappears with them.
#
#   cargo build -p aw-sync --example world_probe
#   tools/testbench/firewall-lan.sh [host|closed]
#
# `host` (default) must end with the player's request arriving.
# `closed` answers no knocks: the player must time out — proof that the
# firewall really blocks and that the knock is what gets through.
set -euo pipefail
mode="${1:-host}"
probe="$(cd "$(dirname "$0")/../.." && pwd)/build/cargo/debug/examples/world_probe"
[ -x "$probe" ] || { echo "build the probe first: cargo build -p aw-sync --example world_probe"; exit 1; }

if [ "${AW_IN_NS:-}" != 1 ]; then
  exec unshare --user --map-root-user --net --mount --fork env AW_IN_NS=1 "$0" "$mode"
fi

firewall() {
  nft -f - <<'EOF'
table inet guard {
  chain input {
    type filter hook input priority 0; policy drop;
    iif lo accept
    ct state established,related accept
    udp dport 5353 accept
  }
}
EOF
}

# This stack is the player; a second one, the host, hangs off a cable.
unshare --net --fork sleep 120 &
sleeper=$!
sleep 0.3
hostns=$(pgrep -P "$sleeper" sleep || echo "$sleeper")
ip link add player0 type veth peer name host0
ip link set host0 netns "$hostns"
ip link set lo up
ip addr add 10.77.0.1/24 dev player0
ip link set player0 up
ip route add 224.0.0.0/4 dev player0
nsenter -t "$hostns" -n sh -c '
  ip link set lo up
  ip addr add 10.77.0.2/24 dev host0
  ip link set host0 up
  ip route add 224.0.0.0/4 dev host0'
firewall
nsenter -t "$hostns" -n bash -c "$(declare -f firewall); firewall"

nsenter -t "$hostns" -n "$probe" "$mode" &
hostpid=$!
sleep 1
set +e
"$probe" player
result=$?
set -e
kill "$hostpid" "$sleeper" 2>/dev/null || true
exit $result
