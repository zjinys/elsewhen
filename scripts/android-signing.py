#!/usr/bin/env python3
"""恢复 CI 签名或核验 APK，只输出公钥指纹，不输出私钥和口令。"""
import argparse
import base64
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def expected_fingerprint():
    value = (ROOT / "ui/android/release-signing.sha256").read_text().strip()
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ValueError("固定签名指纹格式错误")
    return value


def required_env(name):
    value = os.environ.get(name)
    if not value:
        raise ValueError(f"缺少 {name}；不能生成临时签名替代")
    return value


def restore():
    encoded = required_env("ANDROID_KEYSTORE_BASE64")
    target = Path(required_env("ANDROID_KEYSTORE_PATH"))
    alias = required_env("ANDROID_KEY_ALIAS")
    required_env("ANDROID_KEYSTORE_PASSWORD")
    required_env("ANDROID_KEY_PASSWORD")
    data = base64.b64decode(encoded, validate=True)
    if not data or len(data) > 32768:
        raise ValueError("签名密钥文件大小不合法")
    expected = expected_fingerprint()
    # 不覆盖已有文件，避免误把临时 CI 恢复操作用于覆盖本机固定密钥。
    with target.open("xb") as output:
        target.chmod(0o600)
        output.write(data)
    try:
        result = subprocess.run(
            ["keytool", "-exportcert", "-keystore", str(target), "-alias", alias,
             "-storepass:env", "ANDROID_KEYSTORE_PASSWORD"],
            capture_output=True, check=True,
        )
        if hashlib.sha256(result.stdout).hexdigest() != expected:
            raise ValueError("签名证书与固定指纹不同，停止构建")
    except Exception:
        target.unlink()
        raise
    print(f"固定签名已恢复并核验：{expected}")


def verify_apk(apk, signer):
    result = subprocess.run(
        [signer, "verify", "--print-certs", str(apk)],
        capture_output=True, text=True, check=True,
    )
    fingerprints = re.findall(
        r"^Signer #\d+ certificate SHA-256 digest: ([0-9a-f]+)$",
        result.stdout, re.MULTILINE,
    )
    if fingerprints != [expected_fingerprint()]:
        raise ValueError("APK 签名与固定证书不同，不能发布或覆盖更新")
    print(f"APK 固定签名核验通过：{fingerprints[0]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("restore")
    verify = commands.add_parser("verify-apk")
    verify.add_argument("apk", type=Path)
    verify.add_argument("--apksigner", default="apksigner")
    args = parser.parse_args()
    try:
        if args.command == "restore":
            restore()
        else:
            verify_apk(args.apk, args.apksigner)
    except ValueError as error:
        print(f"签名操作失败：{error}", file=sys.stderr)
        return 1
    except (OSError, subprocess.CalledProcessError):
        # 工具 stderr 可能含参数；仅报告可定位且不含密钥内容的失败类型。
        print("签名操作失败：检查签名配置、证书指纹及工具是否可用。", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
