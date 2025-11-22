"""
required for attaching the rust debugger to the vscode extension.

see https://github.com/vadimcn/codelldb/blob/master/MANUAL.md#attach-sequence:~:text=Note%20that%20attaching%20to%20a%20running%20process%20may%20be%20restricted%20on%20some%20systems%2E
"""

from __future__ import annotations

import subprocess
import sys

if sys.platform == "linux":
    result = subprocess.run(
        ["sysctl", "kernel.yama.ptrace_scope", "-n"],
        check=True,
        capture_output=True,
        encoding="utf8",
    )
    if result.stdout.strip() != "0":
        command = ["sudo", "sysctl", "-w", "kernel.yama.ptrace_scope=0"]
        print(
            f"the following command needs to be run to allow debugging rust code on your system: `{' '.join(command)}`"
        )
        _ = subprocess.run(command, check=True)
