#!/usr/bin/env python3
"""Re-apply M3 login string-extraction edits on top of the chizui-login patch.

Run from the upstream worktree root (rebuild_chizui_patch.sh does this).
Idempotent: edits already present are skipped; new deltas are applied once.

If you extract more login strings in the current branch, add the literal ->
stringResource pair to `repls` (and the string to STRINGS_BLOCK), then re-run
scripts/rebuild_chizui_patch.sh. Its parity check warns when the patched
login-string set diverges from the current branch.
"""
import sys

SCREENS = "android/app/src/main/java/com/opencloudgaming/opennow/OpenNowScreens.kt"
STRINGS = "android/app/src/main/res/values/strings.xml"

repls = [
    # --- Login tools dialog ---
    ('title = { Text("Sign-in tools") },',
     'title = { Text(stringResource(R.string.login_tools_title)) },'),
    ('Text("Use a token to sign in without the browser, or export diagnostics before signing in.")',
     'Text(stringResource(R.string.login_tools_description))'),
    ('Text("Sign in with token")\n                    }',
     'Text(stringResource(R.string.login_with_token))\n                    }'),
    ('Text(if (tvLogin) "Export logs with QR" else "Export logs")',
     'Text(if (tvLogin) stringResource(R.string.login_logs_export_qr) else stringResource(R.string.settings_logs_export))'),
    # --- Token dialog ---
    ('title = { Text("Sign in with token") },',
     'title = { Text(stringResource(R.string.login_token_title)) },'),
    ('Text("Paste an NVIDIA access token or token-response JSON. OpenNOW verifies the access token before saving the account.")',
     'Text(stringResource(R.string.login_token_description))'),
    ('label = { Text("Access token") },',
     'label = { Text(stringResource(R.string.login_token_label)) },'),
    ('"Only use credentials for an account you control.",',
     'stringResource(R.string.login_token_warning),'),
    ('Text("Sign in")',
     'Text(stringResource(R.string.login_sign_in))'),
    # --- TvPhoneSignInConnector ---
    ('Text(if (connector.busy) "Starting phone pairing\u2026" else "Sign in from OpenNOW on phone")',
     'Text(if (connector.busy) stringResource(R.string.login_pair_starting) else stringResource(R.string.login_pair_start))'),
    ('if (connector.pairedDeviceName == null) "Pair your phone" else "Phone connected",',
     'if (connector.pairedDeviceName == null) stringResource(R.string.login_pair_title) else stringResource(R.string.login_pair_connected),'),
    ('"Your TV and phone must be on the same Wi-Fi. Scan the QR code with your phone camera; the pairing code expires after five minutes."',
     'stringResource(R.string.login_pair_body)'),
    ('"${connector.pairedDeviceName} can launch games. Approve trust below for settings, overlays, sessions, and account switching."',
     'stringResource(R.string.login_pair_connected_body, connector.pairedDeviceName)'),
    ('label = "Trust this phone",',
     'label = stringResource(R.string.login_pair_trust_label),'),
    ('description = "Required before the phone can transfer an account or control TV settings and sessions.",',
     'description = stringResource(R.string.login_pair_trust_description),'),
    ('Text(if (connector.pairedDeviceName == null) "Cancel pairing" else "Disconnect phone")',
     'Text(if (connector.pairedDeviceName == null) stringResource(R.string.login_pair_cancel) else stringResource(R.string.login_pair_disconnect))'),
    # --- PairingCodeDisplay ---
    ('Text("PAIRING CODE", color = TextMuted, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)',
     'Text(stringResource(R.string.login_pair_code), color = TextMuted, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)'),
    # --- DeviceLoginControls URL messages ---
    ('onUrlActionMessage("Opening sign-in URL")',
     'onUrlActionMessage(context.getString(R.string.login_url_opening))'),
    ('onUrlActionMessage("URL copied")',
     'onUrlActionMessage(context.getString(R.string.login_url_copied))'),
]

# strings.xml block appended after login_logs_export_failed (kept contiguous
# with the base-patch additions). Idempotent via the login_tools_title guard.
STRINGS_BLOCK = """    <string name="login_tools_title">Sign-in tools</string>
    <string name="login_tools_description">Use a token to sign in without the browser, or export diagnostics before signing in.</string>
    <string name="login_with_token">Sign in with token</string>
    <string name="login_token_title">Sign in with token</string>
    <string name="login_token_description">Paste an NVIDIA access token or token-response JSON. OpenNOW verifies the access token before saving the account.</string>
    <string name="login_token_label">Access token</string>
    <string name="login_token_warning">Only use credentials for an account you control.</string>
    <string name="login_pair_start">Sign in from OpenNOW on phone</string>
    <string name="login_pair_starting">Starting phone pairing\u2026</string>
    <string name="login_pair_title">Pair your phone</string>
    <string name="login_pair_connected">Phone connected</string>
    <string name="login_pair_body">Your TV and phone must be on the same Wi-Fi. Scan the QR code with your phone camera; the pairing code expires after five minutes.</string>
    <string name="login_pair_connected_body">%1$s can launch games. Approve trust below for settings, overlays, sessions, and account switching.</string>
    <string name="login_pair_trust_label">Trust this phone</string>
    <string name="login_pair_trust_description">Required before the phone can transfer an account or control TV settings and sessions.</string>
    <string name="login_pair_cancel">Cancel pairing</string>
    <string name="login_pair_disconnect">Disconnect phone</string>
    <string name="login_pair_code">PAIRING CODE</string>
    <string name="login_url_opening">Opening sign-in URL</string>
    <string name="login_url_copied">URL copied</string>
    <string name="login_sign_in">Sign in</string>
    <string name="login_logs_export_qr">Export logs with QR</string>
    <string name="settings_logs_export">Export logs</string>
"""


def main():
    src = open(SCREENS, encoding="utf-8").read()
    applied = skipped = 0
    for old, new in repls:
        if new in src:
            skipped += 1
            continue
        cnt = src.count(old)
        if cnt != 1:
            print(f"FAIL: expected 1 occurrence, found {cnt}:\n  {old!r}")
            sys.exit(1)
        src = src.replace(old, new)
        applied += 1
    open(SCREENS, "w", encoding="utf-8").write(src)
    print(f"screens: {applied} edit(s) applied, {skipped} already present")

    sx = open(STRINGS, encoding="utf-8").read()
    anchor = '    <string name="login_logs_export_failed">Could not export logs</string>'
    if "login_tools_title" in sx:
        print("strings.xml: block already present, skipping")
    else:
        if sx.count(anchor) != 1:
            print("FAIL: strings.xml anchor not found exactly once")
            sys.exit(1)
        sx = sx.replace(anchor, anchor + "\n" + STRINGS_BLOCK.rstrip("\n"))
        open(STRINGS, "w", encoding="utf-8").write(sx)
        print("strings.xml: block appended")


if __name__ == "__main__":
    main()
