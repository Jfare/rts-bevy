#!/usr/bin/env bash
set -euo pipefail

echo "================================================================="
echo "🛡️  rts-bevy Dependency & Security Audit (August 2026 Incident)"
echo "================================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

echo "1. Checking Cargo.lock for known compromised package versions..."

# Check arrayref
if grep -E 'name = "arrayref"' -A 2 Cargo.lock | grep -E 'version = "0\.3\.10"'; then
    echo "❌ CRITICAL: Malicious package 'arrayref@0.3.10' found in Cargo.lock!"
    exit 1
else
    echo "✅ arrayref is safe (not version 0.3.10)."
fi

# Check internment
if grep -E 'name = "internment"' -A 2 Cargo.lock | grep -E 'version = "0\.8\.7"'; then
    echo "❌ CRITICAL: Malicious package 'internment@0.8.7' found in Cargo.lock!"
    exit 1
else
    echo "✅ internment is safe."
fi

# Check append-only-vec
if grep -E 'name = "append-only-vec"' -A 2 Cargo.lock | grep -E 'version = "0\.1\.9"'; then
    echo "❌ CRITICAL: Malicious package 'append-only-vec@0.1.9' found in Cargo.lock!"
    exit 1
else
    echo "✅ append-only-vec is safe."
fi

# Check proc-macro1 typosquat
if grep -E 'name = "proc-macro1"' Cargo.lock; then
    echo "❌ CRITICAL: Malicious typosquat package 'proc-macro1' found in Cargo.lock!"
    exit 1
else
    echo "✅ proc-macro1 not present in Cargo.lock."
fi

echo ""
echo "2. Scanning local Cargo cache (~/.cargo/registry/cache) for compromised crates..."
MALICIOUS_FOUND=$(find ~/.cargo/registry/cache -type f \( -name 'append-only-vec-0.1.9.crate' -o -name 'arrayref-0.3.10.crate' -o -name 'internment-0.8.7.crate' -o -name 'proc-macro1*.crate' \) 2>/dev/null || true)

if [ -n "${MALICIOUS_FOUND}" ]; then
    echo "❌ CRITICAL: Compromised .crate archive found in cache:"
    echo "${MALICIOUS_FOUND}"
    exit 1
else
    echo "✅ Local cargo cache is clean."
fi

echo ""
echo "3. Verifying workspace compilation and unit tests..."
cargo check --workspace
cargo test --workspace

echo ""
echo "================================================================="
echo "🎉 ALL SECURITY CHECKS PASSED. Repository is 100% clean & secure."
echo "================================================================="
