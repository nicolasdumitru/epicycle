#!/usr/bin/env python3
"""Run JavaScript in the app on an emulator or phone, and print the result.

For testing without tapping through the screens, in a debug build (release
builds have no DevTools) that is already open. Run in the Android dev shell,
which `just android-eval` enters by itself. Calling the app's own commands
goes through the same code as the screens:

    just android-eval "window.__TAURI_INTERNALS__.invoke('reminder_settings')"

With several devices connected, choose one by setting ANDROID_SERIAL.
"""

import json
import subprocess
import sys
import urllib.request

import websocket

PACKAGE = "io.github.nicolasdumitru.epicycle"
PORT = 9222


def adb(*args: str) -> str:
    return subprocess.run(["adb", *args], capture_output=True, text=True, check=True).stdout.strip()


def forward_devtools() -> None:
    pid = subprocess.run(["adb", "shell", "pidof", PACKAGE], capture_output=True, text=True).stdout.strip()
    if not pid:
        sys.exit("Epicycle isn't running: start it with `just android-run`.")
    adb("forward", f"tcp:{PORT}", f"localabstract:webview_devtools_remote_{pid}")


def evaluate(expression: str) -> dict:
    pages = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json"))
    socket = websocket.create_connection(pages[0]["webSocketDebuggerUrl"], suppress_origin=True)
    socket.send(json.dumps({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {"expression": expression, "awaitPromise": True, "returnByValue": True},
    }))
    while True:
        message = json.loads(socket.recv())
        if message.get("id") == 1:
            return message["result"]


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    forward_devtools()
    result = evaluate(sys.argv[1])
    if "exceptionDetails" in result:
        sys.exit(json.dumps(result["exceptionDetails"], indent=2))
    print(json.dumps(result["result"].get("value"), indent=2))


if __name__ == "__main__":
    main()
