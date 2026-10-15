#!/usr/bin/env bash
# Sign a locally built Herdr.app with a self-signed identity that stays the
# same on this Mac from one build to the next.
#
# macOS binds a privacy grant (App Management, Full Disk Access, and the rest)
# to the app's code requirement. An ad-hoc signature's requirement is the
# build's own hash, so every rebuild loses the grants and the next tool in a
# Herdr pane is denied. A certificate's requirement is the certificate, which
# survives rebuilds.
#
# The first run creates the identity in its own keychain under
# ~/Library/Keychains, with a random password kept beside it, and adds that
# keychain to the user search list. Each Mac makes its own identity. Deleting
# the keychain and the password file starts over, at the cost of granting the
# permissions again.
#
# usage: bash scripts/sign-local.sh APP
#   HERDR_LOCAL_SIGNING_NAME      the certificate's common name and the
#                                 identity codesign is given (Herdr local signing)
#   HERDR_LOCAL_SIGNING_KEYCHAIN  the keychain file
#                                 (~/Library/Keychains/herdr-local-signing.keychain-db)
set -euo pipefail

fail() { printf 'sign-local.sh: %s\n' "$*" >&2; exit 1; }

[[ $# == 1 ]] || fail 'usage: bash scripts/sign-local.sh APP'
[[ $(uname -s) == Darwin ]] || fail 'macOS required'
[[ -d $1 && -f $1/Contents/Info.plist ]] || fail "not an app bundle: $1"
for tool in security codesign openssl; do
    command -v "$tool" >/dev/null || fail "$tool required"
done

app=$1
name=${HERDR_LOCAL_SIGNING_NAME:-Herdr local signing}
keychain=${HERDR_LOCAL_SIGNING_KEYCHAIN:-$HOME/Library/Keychains/herdr-local-signing.keychain-db}
password_file=${keychain%.keychain-db}.password
[[ $keychain == *.keychain-db ]] || fail 'the keychain path must end in .keychain-db'
[[ $name != *'"'* ]] || fail 'the identity name cannot contain a quote'

# find-identity -v lists only trusted certificates, and this one is self-signed.
has_identity() {
    [[ -f $keychain && -f $password_file ]] \
        && security find-identity -p codesigning "$keychain" 2>/dev/null | grep -Fq "\"$name\""
}

# A subshell: the umask and the temporary directory end with it.
create_identity() (
    umask 077
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/herdr-sign-local.XXXXXX")
    trap 'rm -rf -- "$tmp"' EXIT
    # The check failed, so the keychain or its password is unusable: both start over.
    if [[ -f $keychain ]]; then
        security delete-keychain "$keychain" >/dev/null 2>&1 || rm -f -- "$keychain"
    fi
    rm -f -- "$password_file"
    openssl rand -hex 32 > "$password_file"
    security create-keychain -p "$(<"$password_file")" "$keychain"
    # No arguments: never auto-lock.
    security set-keychain-settings "$keychain"
    cat > "$tmp/openssl.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = codesigning
prompt = no
[dn]
CN = $name
[codesigning]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$tmp/openssl.cnf" \
        -keyout "$tmp/key.pem" -out "$tmp/certificate.pem" 2>/dev/null
    # The legacy PKCS#12 algorithms: `security import` rejects OpenSSL 3's
    # AES default.
    openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/certificate.pem" \
        -out "$tmp/identity.p12" -passout pass:herdr \
        -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
    security import "$tmp/identity.p12" -k "$keychain" -P herdr -T /usr/bin/codesign >/dev/null
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
        -k "$(<"$password_file")" "$keychain" >/dev/null
)

if ! has_identity; then
    create_identity
    has_identity || fail "the identity \"$name\" is missing from $keychain after creating it"
    printf 'sign-local.sh: created the identity "%s" in %s\n' "$name" "$keychain" >&2
fi

# codesign finds identities through the user search list, not --keychain.
# -s replaces the whole list, so the existing entries are passed again.
listed=()
while IFS= read -r line; do
    [[ $line =~ ^[[:space:]]*\"(.*)\"[[:space:]]*$ ]] || continue
    listed+=("${BASH_REMATCH[1]}")
done < <(security list-keychains -d user)
present=0
for entry in "${listed[@]}"; do
    [[ $entry == "$keychain" ]] && present=1
done
if [[ $present == 0 ]]; then
    security list-keychains -d user -s "${listed[@]}" "$keychain"
fi

security unlock-keychain -p "$(<"$password_file")" "$keychain"
codesign --force --sign "$name" --keychain "$keychain" "$app"
codesign --verify --strict "$app"
