#!/bin/bash
# Generate RSA key pair for JWT signing
# This creates a local CA with private key and certificate

set -euo pipefail

KEYS_DIR="certs"
mkdir -p "$KEYS_DIR"

# Generate RSA private key (2048 bits)
openssl genpkey -algorithm RSA -out "$KEYS_DIR/private.pem" -pkeyopt rsa_keygen_bits:2048

# Generate self-signed certificate (acts as local CA)
openssl req -new -x509 -key "$KEYS_DIR/private.pem" \
	-out "$KEYS_DIR/certificate.pem" \
	-days 365 \
	-subj "/CN=SQLite Editor Local CA/O=SQLite Editor/C=US"

# Extract public key from certificate
openssl x509 -in "$KEYS_DIR/certificate.pem" -pubkey -noout >"$KEYS_DIR/public.pem"

echo "Keys generated in $KEYS_DIR/"
echo "  - private.pem: RSA private key for JWT signing"
echo "  - certificate.pem: Self-signed certificate"
echo "  - public.pem: Public key for JWT verification"
