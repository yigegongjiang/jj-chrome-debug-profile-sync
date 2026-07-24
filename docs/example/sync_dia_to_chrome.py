#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import hashlib
import hmac
import json
import os
import shutil
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.padding import PKCS7

DIA_APP = Path("/Applications/Dia.app")
CHROME_APP = Path("/Applications/Google Chrome.app")
DIA_USER_DATA = Path("/Users/hailv/Library/Application Support/Dia/User Data")
CHROME_USER_DATA = Path("/Users/hailv/Library/Application Support/Google/Chrome")
SOURCE_PROFILE = DIA_USER_DATA / "Default"
TARGET_PROFILES = ("Default", "Profile 8")
REPORT_PATH = Path("/Users/hailv/Wrapper/empty/dia-to-chrome-migration-report.json")

DIA_ONLY_EXCLUDES = (
    ".company.thebrowser.dia.*",
    "AgentArtifacts",
    "AgentServer",
    "HomeSurface",
    "Units",
    "agent_artifacts.db*",
    "assistant.db*",
    "assistant_suggestions_database.db*",
    "chat_suggestions_database.db*",
    "custom_skills_database.db*",
    "internal_skills_playground_database.db*",
    "key_value_store.db*",
    "live_data_database.db*",
    "mcp_auth.db*",
    "memory.db*",
    "memory_key_value_store.db*",
    "skills_history_database.db*",
    "sync_queue.db*",
    "tabs.db*",
)

ENCRYPTED_FIELDS = (
    ("Cookies", "cookies", "encrypted_value"),
    ("Extension Cookies", "cookies", "encrypted_value"),
    ("Login Data", "logins", "password_value"),
    ("Web Data", "credit_cards", "card_number_encrypted"),
    ("Web Data", "local_stored_cvc", "value_encrypted"),
    ("Web Data", "keywords", "url_hash"),
)

PROFILE_SHELL_KEYS = (
    "avatar_index",
    "created_by_version",
    "creation_time",
    "family_member_role",
    "managed",
    "managed_user_id",
    "name",
    "using_default_name",
)


def run(
    args: list[str],
    *,
    check: bool = True,
    capture_output: bool = True,
    timeout: int | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        check=check,
        capture_output=capture_output,
        text=True,
        timeout=timeout,
    )


def plist_value(path: Path, key: str) -> str:
    return run(
        ["/usr/libexec/PlistBuddy", "-c", f"Print :{key}", str(path)]
    ).stdout.strip()


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    temporary = path.with_name(f".{path.name}.tmp")
    with temporary.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, ensure_ascii=False, indent=2, sort_keys=True)
        handle.write("\n")
    os.replace(temporary, path)


def browser_processes() -> list[str]:
    output = run(["ps", "ax", "-o", "pid=,command="]).stdout
    markers = ("/Applications/Dia.app/", "/Applications/Google Chrome.app/")
    return [
        line.strip()
        for line in output.splitlines()
        if any(marker in line for marker in markers)
    ]


def keychain_password(service: str, account: str) -> bytes:
    password = run(
        [
            "security",
            "find-generic-password",
            "-w",
            "-s",
            service,
            "-a",
            account,
        ]
    ).stdout.rstrip("\n")
    if not password:
        raise RuntimeError(f"无法读取 {service}")
    return password.encode("utf-8")


def derive_key(password: bytes) -> bytes:
    return hashlib.pbkdf2_hmac("sha1", password, b"saltysalt", 1003, 16)


def decrypt_v10(blob: bytes, key: bytes) -> bytes:
    if not blob.startswith(b"v10"):
        raise ValueError("不是 v10 密文")
    decryptor = Cipher(algorithms.AES(key), modes.CBC(b" " * 16)).decryptor()
    padded = decryptor.update(blob[3:]) + decryptor.finalize()
    unpadder = PKCS7(128).unpadder()
    return unpadder.update(padded) + unpadder.finalize()


def encrypt_v10(plaintext: bytes, key: bytes) -> bytes:
    padder = PKCS7(128).padder()
    padded = padder.update(plaintext) + padder.finalize()
    encryptor = Cipher(algorithms.AES(key), modes.CBC(b" " * 16)).encryptor()
    return b"v10" + encryptor.update(padded) + encryptor.finalize()


def prune_empty_json(value: Any) -> Any:
    if isinstance(value, dict):
        pruned = {key: prune_empty_json(item) for key, item in value.items()}
        return {
            key: item
            for key, item in pruned.items()
            if not (isinstance(item, (dict, list)) and not item)
        }
    if isinstance(value, list):
        pruned = [prune_empty_json(item) for item in value]
        return [
            item for item in pruned if not (isinstance(item, (dict, list)) and not item)
        ]
    return value


def chromium_json(value: Any, *, exists: bool = True) -> bytes:
    if not exists:
        return b""
    if isinstance(value, dict):
        value = prune_empty_json(value)
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    encoded = (
        encoded.replace("<", "\\u003C")
        .replace("\u2028", "\\u2028")
        .replace("\u2029", "\\u2029")
    )
    return encoded.encode("utf-8")


def machine_id() -> bytes:
    output = run(["ioreg", "-rd1", "-c", "IOPlatformExpertDevice"]).stdout
    for line in output.splitlines():
        if "IOPlatformUUID" in line:
            return line.split("=", 1)[1].strip().strip('"').encode("utf-8")
    raise RuntimeError("无法读取 IOPlatformUUID")


def datapack_resources(path: Path) -> dict[int, bytes]:
    data = path.read_bytes()
    version = struct.unpack_from("<I", data, 0)[0]
    if version == 5:
        _, resource_count, _ = struct.unpack_from("<BxxxHH", data, 4)
        header_size = 12
    elif version == 4:
        resource_count, _ = struct.unpack_from("<IB", data, 4)
        header_size = 9
    else:
        raise RuntimeError(f"未知 Chrome DataPack 版本：{version}")
    entries = [
        struct.unpack_from("<HI", data, header_size + index * 6)
        for index in range(resource_count + 1)
    ]
    return {
        entries[index][0]: data[entries[index][1] : entries[index + 1][1]]
        for index in range(resource_count)
    }


def find_chrome_pref_hash_seed(device_id: bytes) -> tuple[int, bytes]:
    secure = load_json(CHROME_USER_DATA / "Default" / "Secure Preferences")
    protection = secure["protection"]
    expected = protection["super_mac"]
    serialized_macs = chromium_json(protection["macs"])
    pak = (
        CHROME_APP
        / "Contents/Frameworks/Google Chrome Framework.framework/Versions/Current/Resources/resources.pak"
    )
    matches: list[tuple[int, bytes]] = []
    for resource_id, candidate in datapack_resources(pak).items():
        if not 1 <= len(candidate) <= 1024:
            continue
        digest = (
            hmac.new(
                candidate,
                device_id + serialized_macs,
                hashlib.sha256,
            )
            .hexdigest()
            .upper()
        )
        if digest == expected:
            matches.append((resource_id, candidate))
    if len(matches) != 1:
        raise RuntimeError(f"Chrome Pref Hash Seed 匹配数异常：{len(matches)}")
    return matches[0]


def nested_value(root: dict[str, Any], parts: tuple[str, ...]) -> tuple[Any, bool]:
    current: Any = root
    for part in parts:
        if not isinstance(current, dict) or part not in current:
            return None, False
        current = current[part]
    return current, True


def leaf_items(
    root: dict[str, Any],
    prefix: tuple[str, ...] = (),
) -> list[tuple[tuple[str, ...], Any]]:
    result: list[tuple[tuple[str, ...], Any]] = []
    for key, value in root.items():
        path = prefix + (key,)
        if isinstance(value, dict):
            result.extend(leaf_items(value, path))
        else:
            result.append((path, value))
    return result


def set_nested(root: dict[str, Any], parts: tuple[str, ...], value: Any) -> None:
    current = root
    for part in parts[:-1]:
        current = current.setdefault(part, {})
    current[parts[-1]] = value


def filter_encrypted_hashes(root: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in root.items():
        if key.endswith("_encrypted_hash"):
            result[key] = value
        elif isinstance(value, dict):
            filtered = filter_encrypted_hashes(value)
            if filtered:
                result[key] = filtered
    return result


def secure_pref_digest(
    seed: bytes,
    pref_path: str,
    serialized_value: bytes,
) -> bytes:
    return hashlib.sha256(seed + pref_path.encode("utf-8") + serialized_value).digest()


def resign_secure_preferences(
    profile: Path,
    seed: bytes,
    device_id: bytes,
    chrome_key: bytes,
) -> dict[str, int]:
    path = profile / "Secure Preferences"
    secure = load_json(path)
    protection = secure["protection"]
    macs = protection["macs"]
    legacy_count = 0
    encrypted_count = 0

    for stored_path, _ in leaf_items(macs):
        encrypted = any(part.endswith("_encrypted_hash") for part in stored_path)
        actual_path = tuple(
            part.removesuffix("_encrypted_hash") for part in stored_path
        )
        pref_path = ".".join(actual_path)
        value, exists = nested_value(secure, actual_path)
        serialized_value = chromium_json(value, exists=exists)
        if encrypted:
            digest = secure_pref_digest(seed, pref_path, serialized_value)
            replacement = base64.b64encode(encrypt_v10(digest, chrome_key)).decode(
                "ascii"
            )
            encrypted_count += 1
        else:
            replacement = (
                hmac.new(
                    seed,
                    device_id + pref_path.encode("utf-8") + serialized_value,
                    hashlib.sha256,
                )
                .hexdigest()
                .upper()
            )
            legacy_count += 1
        set_nested(macs, stored_path, replacement)

    protection["super_mac"] = (
        hmac.new(
            seed,
            device_id + chromium_json(macs),
            hashlib.sha256,
        )
        .hexdigest()
        .upper()
    )
    filtered = filter_encrypted_hashes(macs)
    protection["super_encrypted_hash"] = base64.b64encode(
        encrypt_v10(
            secure_pref_digest(seed, "", chromium_json(filtered)),
            chrome_key,
        )
    ).decode("ascii")
    write_json(path, secure)
    return {"legacy_macs": legacy_count, "encrypted_hashes": encrypted_count}


def verify_secure_preferences(
    profile: Path,
    seed: bytes,
    device_id: bytes,
    chrome_key: bytes,
) -> dict[str, int]:
    secure = load_json(profile / "Secure Preferences")
    protection = secure["protection"]
    macs = protection["macs"]
    legacy_count = 0
    encrypted_count = 0

    for stored_path, stored in leaf_items(macs):
        encrypted = any(part.endswith("_encrypted_hash") for part in stored_path)
        actual_path = tuple(
            part.removesuffix("_encrypted_hash") for part in stored_path
        )
        pref_path = ".".join(actual_path)
        value, exists = nested_value(secure, actual_path)
        serialized_value = chromium_json(value, exists=exists)
        if encrypted:
            expected = secure_pref_digest(seed, pref_path, serialized_value)
            actual = decrypt_v10(base64.b64decode(stored), chrome_key)
            if actual != expected:
                raise RuntimeError(f"Secure Preferences 加密哈希失败：{pref_path}")
            encrypted_count += 1
        else:
            expected = (
                hmac.new(
                    seed,
                    device_id + pref_path.encode("utf-8") + serialized_value,
                    hashlib.sha256,
                )
                .hexdigest()
                .upper()
            )
            if stored != expected:
                raise RuntimeError(f"Secure Preferences HMAC 失败：{pref_path}")
            legacy_count += 1

    expected_super_mac = (
        hmac.new(
            seed,
            device_id + chromium_json(macs),
            hashlib.sha256,
        )
        .hexdigest()
        .upper()
    )
    if protection["super_mac"] != expected_super_mac:
        raise RuntimeError("Secure Preferences Super MAC 失败")
    filtered = filter_encrypted_hashes(macs)
    expected_super_hash = secure_pref_digest(seed, "", chromium_json(filtered))
    actual_super_hash = decrypt_v10(
        base64.b64decode(protection["super_encrypted_hash"]),
        chrome_key,
    )
    if actual_super_hash != expected_super_hash:
        raise RuntimeError("Secure Preferences Super Encrypted Hash 失败")
    return {"legacy_macs": legacy_count, "encrypted_hashes": encrypted_count}


def qident(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def table_exists(db: sqlite3.Connection, table: str) -> bool:
    return (
        db.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            (table,),
        ).fetchone()
        is not None
    )


def column_exists(db: sqlite3.Connection, table: str, column: str) -> bool:
    if not table_exists(db, table):
        return False
    return any(
        row[1] == column for row in db.execute(f"PRAGMA table_info({qident(table)})")
    )


def reencrypt_profile(
    profile: Path, source_key: bytes, target_key: bytes
) -> dict[str, int]:
    counts: dict[str, int] = {}
    grouped: dict[str, list[tuple[str, str]]] = {}
    for database, table, column in ENCRYPTED_FIELDS:
        grouped.setdefault(database, []).append((table, column))

    for database, fields in grouped.items():
        path = profile / database
        if not path.exists():
            continue
        db = sqlite3.connect(path)
        try:
            for table, column in fields:
                if not column_exists(db, table, column):
                    continue
                select_sql = (
                    f"SELECT rowid, {qident(column)} FROM {qident(table)} "
                    f"WHERE typeof({qident(column)})='blob' "
                    f"AND hex(substr({qident(column)},1,3))='763130'"
                )
                rows = list(db.execute(select_sql))
                update_sql = (
                    f"UPDATE {qident(table)} SET {qident(column)}=? WHERE rowid=?"
                )
                for rowid, ciphertext in rows:
                    plaintext = decrypt_v10(ciphertext, source_key)
                    db.execute(
                        update_sql,
                        (sqlite3.Binary(encrypt_v10(plaintext, target_key)), rowid),
                    )
                counts[f"{database}:{table}.{column}"] = len(rows)
            db.commit()
            db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        finally:
            db.close()
    return counts


def verify_encryption(profile: Path, key: bytes) -> dict[str, int]:
    counts: dict[str, int] = {}
    for database, table, column in ENCRYPTED_FIELDS:
        path = profile / database
        if not path.exists():
            continue
        db = sqlite3.connect(f"file:{path}?mode=ro&immutable=1", uri=True)
        try:
            if not column_exists(db, table, column):
                continue
            extra = ", host_key" if database in ("Cookies", "Extension Cookies") else ""
            sql = (
                f"SELECT {qident(column)}{extra} FROM {qident(table)} "
                f"WHERE typeof({qident(column)})='blob' "
                f"AND hex(substr({qident(column)},1,3))='763130'"
            )
            verified = 0
            for row in db.execute(sql):
                plaintext = decrypt_v10(row[0], key)
                if (
                    extra
                    and plaintext[:32]
                    != hashlib.sha256(row[1].encode("utf-8")).digest()
                ):
                    raise RuntimeError(f"{database} Cookie host hash 校验失败")
                verified += 1
            counts[f"{database}:{table}.{column}"] = verified
        finally:
            db.close()
    return counts


def sqlite_count(path: Path, table: str, where: str = "") -> int:
    if not path.exists():
        return 0
    db = sqlite3.connect(f"file:{path}?mode=ro&immutable=1", uri=True)
    try:
        if not table_exists(db, table):
            return 0
        suffix = f" WHERE {where}" if where else ""
        return int(
            db.execute(f"SELECT count(*) FROM {qident(table)}{suffix}").fetchone()[0]
        )
    finally:
        db.close()


def bookmarks_count(path: Path) -> int:
    if not path.exists():
        return 0
    data = load_json(path)

    def visit(node: Any) -> int:
        if isinstance(node, dict):
            own = 1 if node.get("type") == "url" else 0
            return own + sum(visit(value) for value in node.values())
        if isinstance(node, list):
            return sum(visit(value) for value in node)
        return 0

    return visit(data.get("roots", {}))


def directory_size(path: Path) -> int:
    total = 0
    if not path.exists():
        return total
    for root, _, files in os.walk(path):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except FileNotFoundError:
                pass
    return total


def extension_ids(profile: Path) -> list[str]:
    directory = profile / "Extensions"
    if not directory.exists():
        return []
    return sorted(
        item.name
        for item in directory.iterdir()
        if item.is_dir()
        and len(item.name) == 32
        and set(item.name) <= set("abcdefghijklmnop")
    )


def registered_extension_ids(profile: Path) -> list[str]:
    path = profile / "Secure Preferences"
    if not path.exists():
        return []
    settings = load_json(path).get("extensions", {}).get("settings", {})
    return sorted(settings)


def metrics(profile: Path) -> dict[str, Any]:
    chrome_now = int(time.time() * 1_000_000) + 11_644_473_600_000_000
    return {
        "bookmarks": bookmarks_count(profile / "Bookmarks"),
        "cookies": sqlite_count(profile / "Cookies", "cookies"),
        "cookies_persistent": sqlite_count(
            profile / "Cookies", "cookies", "is_persistent=1"
        ),
        "cookies_live_persistent": sqlite_count(
            profile / "Cookies",
            "cookies",
            f"is_persistent=1 AND expires_utc>{chrome_now}",
        ),
        "cookies_expired_persistent": sqlite_count(
            profile / "Cookies",
            "cookies",
            f"is_persistent=1 AND expires_utc<={chrome_now}",
        ),
        "cookies_session": sqlite_count(
            profile / "Cookies", "cookies", "is_persistent=0"
        ),
        "downloads": sqlite_count(profile / "History", "downloads"),
        "extensions": extension_ids(profile),
        "extensions_registered": registered_extension_ids(profile),
        "history_urls": sqlite_count(profile / "History", "urls"),
        "history_visits": sqlite_count(profile / "History", "visits"),
        "passwords": sqlite_count(profile / "Login Data", "logins"),
        "sessions": len(list((profile / "Sessions").glob("*")))
        if (profile / "Sessions").exists()
        else 0,
        "site_storage_bytes": sum(
            directory_size(profile / name)
            for name in (
                "File System",
                "IndexedDB",
                "Local Storage",
                "Service Worker",
                "Session Storage",
                "WebStorage",
            )
        ),
        "webdata_autofill_profiles": sqlite_count(
            profile / "Web Data", "autofill_profiles"
        ),
        "webdata_credit_cards": sqlite_count(profile / "Web Data", "credit_cards"),
    }


def integrity_check(profile: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for name in (
        "Cookies",
        "Extension Cookies",
        "Favicons",
        "History",
        "Login Data",
        "Web Data",
    ):
        path = profile / name
        if not path.exists():
            continue
        db = sqlite3.connect(f"file:{path}?mode=ro&immutable=1", uri=True)
        try:
            value = str(db.execute("PRAGMA integrity_check").fetchone()[0])
        finally:
            db.close()
        if value != "ok":
            raise RuntimeError(f"{path}: integrity_check={value}")
        result[name] = value
    return result


def copy_source_to_base(base: Path) -> None:
    args = ["/usr/bin/rsync", "-aE"]
    for pattern in DIA_ONLY_EXCLUDES:
        args.extend(["--exclude", pattern])
    args.extend([f"{SOURCE_PROFILE}/", f"{base}/"])
    run(args, capture_output=False)


def clone_directory(source: Path, target: Path) -> None:
    run(["/bin/cp", "-cR", str(source), str(target)], capture_output=False)


def patch_profile_shell(
    profile: Path, old_profile: Path, cached_name: str | None
) -> None:
    source_preferences = load_json(profile / "Preferences")
    old_preferences = load_json(old_profile / "Preferences")
    source_profile = source_preferences.setdefault("profile", {})
    old_profile_preferences = old_preferences.get("profile", {})
    for key in PROFILE_SHELL_KEYS:
        if key in old_profile_preferences:
            source_profile[key] = old_profile_preferences[key]
    if cached_name:
        source_profile["name"] = cached_name
    if "enterprise_profile_guid" in old_preferences:
        source_preferences["enterprise_profile_guid"] = old_preferences[
            "enterprise_profile_guid"
        ]
    source_profile["exit_type"] = "Crashed"
    source_profile["exited_cleanly"] = False
    write_json(profile / "Preferences", source_preferences)


def smoke_test(base: Path, temporary_root: Path) -> dict[str, Any]:
    smoke_user_data = temporary_root / "smoke-user-data"
    smoke_user_data.mkdir()
    clone_directory(base, smoke_user_data / "Default")
    shutil.copy2(DIA_USER_DATA / "Local State", smoke_user_data / "Local State")
    before = metrics(smoke_user_data / "Default")
    executable = CHROME_APP / "Contents/MacOS/Google Chrome"
    process = subprocess.Popen(
        [
            str(executable),
            "--headless=new",
            f"--user-data-dir={smoke_user_data}",
            "--profile-directory=Default",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--remote-debugging-port=0",
            "about:blank",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    ready_file = smoke_user_data / "DevToolsActivePort"
    deadline = time.monotonic() + 60
    ready = False
    try:
        while time.monotonic() < deadline:
            if ready_file.exists():
                ready = True
                break
            returncode = process.poll()
            if returncode is not None:
                raise RuntimeError(f"Chrome 隔离启动失败，exit={returncode}")
            time.sleep(0.25)
        if not ready:
            raise RuntimeError("Chrome 隔离启动 60 秒内未就绪")
        time.sleep(2)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)

    after = metrics(smoke_user_data / "Default")
    if after["history_urls"] < before["history_urls"]:
        raise RuntimeError("Chrome 隔离启动后 History 记录减少")
    if after["passwords"] < before["passwords"]:
        raise RuntimeError("Chrome 隔离启动后密码记录减少")
    if after["cookies_live_persistent"] < before["cookies_live_persistent"]:
        raise RuntimeError("Chrome 隔离启动后未过期 Cookie 记录减少")
    if not set(before["extensions"]).issubset(after["extensions_registered"]):
        raise RuntimeError("Chrome 隔离启动后 Dia 扩展注册丢失")
    return {
        "ready": ready,
        "returncode": process.returncode,
        "before": before,
        "after": after,
        "integrity": integrity_check(smoke_user_data / "Default"),
    }


def replace_profiles(
    base: Path,
    temporary_root: Path,
    local_state: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    prepared: dict[str, Path] = {}
    old_paths: dict[str, Path] = {}
    report: dict[str, dict[str, Any]] = {}
    info_cache = local_state.get("profile", {}).get("info_cache", {})

    for index, name in enumerate(TARGET_PROFILES):
        target = CHROME_USER_DATA / name
        prepared_path = temporary_root / f"prepared-{index}"
        clone_directory(base, prepared_path)
        cached_name = info_cache.get(name, {}).get("name")
        patch_profile_shell(prepared_path, target, cached_name)
        report[name] = {
            "prepared_metrics": metrics(prepared_path),
            "prepared_integrity": integrity_check(prepared_path),
            "encryption": verify_encryption(
                prepared_path,
                derive_key(keychain_password("Chrome Safe Storage", "Chrome")),
            ),
        }
        prepared[name] = prepared_path

    swapped: list[str] = []
    try:
        for index, name in enumerate(TARGET_PROFILES):
            target = CHROME_USER_DATA / name
            old_path = temporary_root / f"old-{index}"
            os.replace(target, old_path)
            old_paths[name] = old_path
            os.replace(prepared[name], target)
            swapped.append(name)

        for name in TARGET_PROFILES:
            report[name]["final_metrics"] = metrics(CHROME_USER_DATA / name)
            report[name]["final_integrity"] = integrity_check(CHROME_USER_DATA / name)
            report[name]["final_encryption"] = verify_encryption(
                CHROME_USER_DATA / name,
                derive_key(keychain_password("Chrome Safe Storage", "Chrome")),
            )
    except Exception:
        for name in reversed(swapped):
            target = CHROME_USER_DATA / name
            failed = temporary_root / f"failed-{name.replace(' ', '-')}"
            if target.exists():
                os.replace(target, failed)
            os.replace(old_paths[name], target)
        raise

    for old_path in old_paths.values():
        shutil.rmtree(old_path)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(
        description="将 Dia Default Profile 覆盖同步到两个 Chrome Profile。"
    )
    parser.add_argument("--execute", action="store_true", help="实际执行覆盖迁移")
    args = parser.parse_args()

    if not args.execute:
        parser.error("必须显式传入 --execute")

    processes = browser_processes()
    if processes:
        raise RuntimeError("浏览器仍在运行：\n" + "\n".join(processes))

    dia_version = plist_value(
        DIA_APP
        / "Contents/Frameworks/ArcCore.framework/Versions/A/Resources/Info.plist",
        "CFBundleShortVersionString",
    )
    chrome_version = plist_value(
        CHROME_APP
        / "Contents/Frameworks/Google Chrome Framework.framework/Versions/Current/Resources/Info.plist",
        "CFBundleShortVersionString",
    )
    if dia_version.split(".")[:3] != chrome_version.split(".")[:3]:
        raise RuntimeError(
            f"内核版本不匹配：Dia={dia_version}, Chrome={chrome_version}"
        )

    for name in TARGET_PROFILES:
        if not (CHROME_USER_DATA / name / "Preferences").exists():
            raise RuntimeError(f"目标 Profile 不存在：{name}")

    source_integrity = integrity_check(SOURCE_PROFILE)
    source_metrics = metrics(SOURCE_PROFILE)
    source_key = derive_key(keychain_password("Dia Safe Storage", "Dia"))
    chrome_key = derive_key(keychain_password("Chrome Safe Storage", "Chrome"))
    if source_key == chrome_key:
        raise RuntimeError("Dia 与 Chrome 加密密钥意外相同，停止迁移")
    source_encryption = verify_encryption(SOURCE_PROFILE, source_key)
    device_id = machine_id()
    seed_resource_id, chrome_pref_hash_seed = find_chrome_pref_hash_seed(device_id)

    local_state = load_json(CHROME_USER_DATA / "Local State")
    temporary_root = Path(
        tempfile.mkdtemp(prefix=".dia-to-chrome-", dir=CHROME_USER_DATA)
    )
    try:
        base = temporary_root / "base"
        base.mkdir()
        copy_source_to_base(base)
        rewritten = reencrypt_profile(base, source_key, chrome_key)
        resigned_secure_preferences = resign_secure_preferences(
            base,
            chrome_pref_hash_seed,
            device_id,
            chrome_key,
        )
        base_encryption = verify_encryption(base, chrome_key)
        if rewritten != base_encryption or rewritten != source_encryption:
            raise RuntimeError("密文转换计数不一致")
        secure_preferences_verification = verify_secure_preferences(
            base,
            chrome_pref_hash_seed,
            device_id,
            chrome_key,
        )
        if secure_preferences_verification != resigned_secure_preferences:
            raise RuntimeError("Secure Preferences 重签计数不一致")
        base_integrity = integrity_check(base)
        smoke = smoke_test(base, temporary_root)
        targets = replace_profiles(base, temporary_root, local_state)

        expected = metrics(base)
        for name in TARGET_PROFILES:
            final_metrics = targets[name]["final_metrics"]
            for key in (
                "bookmarks",
                "cookies",
                "downloads",
                "extensions",
                "extensions_registered",
                "history_urls",
                "history_visits",
                "passwords",
                "sessions",
                "site_storage_bytes",
                "webdata_autofill_profiles",
                "webdata_credit_cards",
            ):
                if final_metrics[key] != expected[key]:
                    raise RuntimeError(f"{name} 的 {key} 与 Dia 不一致")

        report = {
            "status": "success",
            "dia_chromium_version": dia_version,
            "chrome_version": chrome_version,
            "source_profile": str(SOURCE_PROFILE),
            "target_profiles": [
                str(CHROME_USER_DATA / name) for name in TARGET_PROFILES
            ],
            "source_metrics": source_metrics,
            "source_integrity": source_integrity,
            "source_encryption": source_encryption,
            "base_integrity": base_integrity,
            "reencrypted_fields": rewritten,
            "secure_preferences": {
                "chrome_seed_resource_id": seed_resource_id,
                "chrome_seed_sha256": hashlib.sha256(chrome_pref_hash_seed).hexdigest(),
                "resigned": resigned_secure_preferences,
                "verified": secure_preferences_verification,
            },
            "smoke_test": smoke,
            "targets": targets,
            "excluded_dia_only_patterns": list(DIA_ONLY_EXCLUDES),
            "chrome_backup_retained": False,
        }
        write_json(REPORT_PATH, report)
    finally:
        if temporary_root.exists():
            shutil.rmtree(temporary_root)

    print(
        json.dumps(
            {"status": "success", "report": str(REPORT_PATH)}, ensure_ascii=False
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(
            json.dumps({"status": "failed", "error": str(error)}, ensure_ascii=False),
            file=sys.stderr,
        )
        raise
