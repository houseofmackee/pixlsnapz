# Shared by bundle.sh and make_dmg.sh (sourced, not run).
#
# Signing identity, in order:
#   1. SIGN_IDENTITY, if set ("-" for ad hoc)
#   2. a valid "Developer ID Application" identity in the keychain (of team
#      PIXLSNAPZ_TEAM_ID, if set): releases are signed this way and notarized
#   3. the self-signed "PixlSnapz Release Signing" certificate (local builds
#      without a Developer ID)
#   4. ad hoc
#
# macOS ties the Screen Recording permission to the signature's designated
# requirement. With a Developer ID that is the bundle id plus the team, so
# the permission survives updates; ad-hoc signatures change with every build.

RELEASE_IDENTITY="PixlSnapz Release Signing"

developer_id() {
    security find-identity -v -p codesigning 2>/dev/null |
        sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' |
        while read -r name; do
            if [ -z "${PIXLSNAPZ_TEAM_ID:-}" ] || [ "${name%"($PIXLSNAPZ_TEAM_ID)"}" != "$name" ]; then
                echo "$name"
                break
            fi
        done
}

choose_identity() {
    if [ -n "${SIGN_IDENTITY:-}" ]; then
        return
    fi
    SIGN_IDENTITY=$(developer_id)
    if [ -n "$SIGN_IDENTITY" ]; then
        return
    fi
    if security find-identity -p codesigning | grep -q "\"$RELEASE_IDENTITY\""; then
        SIGN_IDENTITY=$RELEASE_IDENTITY
    else
        echo "warning: no Developer ID or \"$RELEASE_IDENTITY\" identity in the keychain; signing ad hoc" >&2
        echo "warning: users will have to grant Screen Recording permission again" >&2
        SIGN_IDENTITY=-
    fi
}

is_developer_id() {
    case "$SIGN_IDENTITY" in "Developer ID Application:"*) return 0 ;; *) return 1 ;; esac
}

# Notarization needs a Developer ID and an App Store Connect API key:
#   PIXLSNAPZ_NOTARY_KEY      path to the .p8 key
#   PIXLSNAPZ_NOTARY_KEY_ID   its key id
#   PIXLSNAPZ_NOTARY_ISSUER   the issuer id
can_notarize() {
    is_developer_id && [ -n "${PIXLSNAPZ_NOTARY_KEY:-}" ] && [ -n "${PIXLSNAPZ_NOTARY_KEY_ID:-}" ] &&
        [ -n "${PIXLSNAPZ_NOTARY_ISSUER:-}" ]
}

# notarize FILE: submits a .zip or .dmg and waits; prints Apple's log and
# fails unless it is accepted.
notarize() {
    out=$(mktemp)
    xcrun notarytool submit "$1" --key "$PIXLSNAPZ_NOTARY_KEY" --key-id "$PIXLSNAPZ_NOTARY_KEY_ID" \
        --issuer "$PIXLSNAPZ_NOTARY_ISSUER" --wait --output-format json > "$out" || true
    status=$(plutil -extract status raw -o - "$out" 2>/dev/null || echo "submission failed")
    id=$(plutil -extract id raw -o - "$out" 2>/dev/null || true)
    rm -f "$out"
    if [ "$status" != "Accepted" ]; then
        echo "error: notarization of $1: $status" >&2
        if [ -n "$id" ]; then
            xcrun notarytool log "$id" --key "$PIXLSNAPZ_NOTARY_KEY" --key-id "$PIXLSNAPZ_NOTARY_KEY_ID" \
                --issuer "$PIXLSNAPZ_NOTARY_ISSUER" >&2 || true
        fi
        return 1
    fi
    echo "Notarized $1"
}

# staple FILE: attaches the notarization ticket. Right after notarization
# the ticket may not be published yet (stapler exits with 65 or 68), so
# this retries for a while.
staple() {
    for attempt in 1 2 3 4 5 6; do
        xcrun stapler staple -q "$1" && return 0
        echo "stapling $1 failed (attempt $attempt); retrying" >&2
        sleep 15
    done
    xcrun stapler staple "$1"
}

# validate_ticket FILE: checks the stapled ticket, retrying like staple
# (the lookup can fail with 68 until the ticket is published).
validate_ticket() {
    for attempt in 1 2 3 4 5 6; do
        xcrun stapler validate -q "$1" && return 0
        echo "validating the ticket of $1 failed (attempt $attempt); retrying" >&2
        sleep 15
    done
    xcrun stapler validate "$1"
}

# sign_with_identity PATH: the hardened runtime, plus a secure timestamp for
# a Developer ID (Apple's timestamp service only accepts its certificates).
sign_with_identity() {
    if is_developer_id; then
        codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$1"
    else
        codesign --force --options runtime --timestamp=none --sign "$SIGN_IDENTITY" "$1"
    fi
}
