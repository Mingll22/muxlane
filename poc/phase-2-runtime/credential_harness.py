#!/usr/bin/env python3
"""Non-production credential transaction harness for the Phase 2 Runtime POC."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import secrets
import stat
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


IDENTIFIER_PATTERN = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
BUFFER_SIZE = 1024 * 1024
MAX_JSON_BYTES = 16 * 1024 * 1024
APPROVED_FILESYSTEM_TYPES = {
    "btrfs",
    "ext2",
    "ext3",
    "ext4",
    "overlay",
    "tmpfs",
    "xfs",
    "zfs",
}
REPOSITORY_ROOT = Path(__file__).absolute().parents[2]


class HarnessError(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


@dataclass(frozen=True)
class FileMetadata:
    sha256: str
    size: int
    mtime_ns: int
    mode: int
    owner_matches: bool

    def as_dict(self) -> dict[str, Any]:
        return {
            "sha256": self.sha256,
            "size": self.size,
            "mtime_ns": self.mtime_ns,
            "mode": self.mode,
            "owner_matches": self.owner_matches,
        }


class CredentialHarness:
    def __init__(self, poc_root: Path):
        raw_root = Path(poc_root)
        if not raw_root.is_absolute() or ".." in raw_root.parts:
            raise HarnessError("PATH_REJECTED", "POC root path is not allowed")
        self.poc_root = Path(os.path.normpath(os.fspath(raw_root)))
        self.current_uid = os.getuid()
        self._validate_poc_root_location()
        self._validate_directory(self.poc_root, 0o700)
        self._validate_filesystem_type(self.poc_root)

    def import_credential(
        self, source_root: Path, source_auth: Path, account: str
    ) -> dict[str, Any]:
        account = self._validate_identifier(account, "account")
        source_root = Path(os.path.abspath(os.fspath(source_root)))
        source_auth = Path(os.path.abspath(os.fspath(source_auth)))
        self._validate_source(source_root, source_auth)
        account_directory = self._poc_path("accounts", account)
        self._validate_directory(account_directory, 0o700)

        source_fd = self._open_source_file(source_auth)
        account_fd = self._open_directory(account_directory, 0o700)
        try:
            source_metadata = self._metadata_for_fd(source_fd)
            self._atomic_copy(
                source_fd,
                account_fd,
                "auth.json",
                overwrite=False,
            )
            vault_fd = self._open_regular_file_at(account_fd, "auth.json", 0o600)
            try:
                vault_metadata = self._metadata_for_fd(vault_fd)
            finally:
                os.close(vault_fd)
        finally:
            os.close(account_fd)
            os.close(source_fd)

        if source_metadata.sha256 != vault_metadata.sha256:
            raise HarnessError("IMPORT_VERIFY_FAILED", "imported credential verification failed")
        return {"status": "imported", "account": account}

    def checkout(self, account: str, project: str) -> dict[str, Any]:
        account = self._validate_identifier(account, "account")
        project = self._validate_identifier(project, "project")
        account_directory = self._poc_path("accounts", account)
        runtime_directory = self._poc_path("projects", project, "codex-home")
        manifest_directory = self._poc_path("manifests")
        self._validate_directory(account_directory, 0o700)
        self._validate_directory(runtime_directory, 0o700)
        self._validate_directory(manifest_directory, 0o700)

        account_fd = self._open_directory(account_directory, 0o700)
        runtime_fd = self._open_directory(runtime_directory, 0o700)
        manifest_fd = self._open_directory(manifest_directory, 0o700)
        manifest_id = f"checkout-{secrets.token_hex(12)}"
        try:
            self._assert_missing_at(runtime_fd, "auth.json", "RUNTIME_AUTH_EXISTS")
            vault_fd = self._open_regular_file_at(account_fd, "auth.json", 0o600)
            try:
                vault_metadata = self._metadata_for_fd(vault_fd)
                created_at = self._utc_now()
                manifest = {
                    "schema": 1,
                    "non_production": True,
                    "manifest_id": manifest_id,
                    "account": account,
                    "project": project,
                    "state": "preparing",
                    "created_at": created_at,
                    "updated_at": created_at,
                    "vault_before_checkout": vault_metadata.as_dict(),
                }
                self._write_manifest(manifest_fd, manifest_id, manifest, overwrite=False)
                self._atomic_copy(vault_fd, runtime_fd, "auth.json", overwrite=False)
            finally:
                os.close(vault_fd)

            runtime_auth_fd = self._open_regular_file_at(runtime_fd, "auth.json", 0o600)
            try:
                runtime_metadata = self._metadata_for_fd(runtime_auth_fd)
            finally:
                os.close(runtime_auth_fd)
            if runtime_metadata.sha256 != vault_metadata.sha256:
                raise HarnessError(
                    "CHECKOUT_VERIFY_FAILED", "runtime credential verification failed"
                )
            manifest["runtime_at_checkout"] = runtime_metadata.as_dict()
            manifest["state"] = "checked_out"
            manifest["updated_at"] = self._utc_now()
            self._write_manifest(manifest_fd, manifest_id, manifest, overwrite=True)
        finally:
            os.close(manifest_fd)
            os.close(runtime_fd)
            os.close(account_fd)
        return {"status": "checked_out", "manifest_id": manifest_id}

    def commit(self, manifest_id: str) -> dict[str, Any]:
        manifest_id = self._validate_manifest_id(manifest_id)
        manifest_directory = self._poc_path("manifests")
        backup_directory = self._poc_path("backups")
        self._validate_directory(manifest_directory, 0o700)
        self._validate_directory(backup_directory, 0o700)

        manifest_fd = self._open_directory(manifest_directory, 0o700)
        backup_fd = self._open_directory(backup_directory, 0o700)
        try:
            manifest = self._read_manifest(manifest_fd, manifest_id)
            if manifest.get("state") != "checked_out":
                raise HarnessError(
                    "INVALID_MANIFEST_STATE", "manifest is not ready for commit"
                )
            account = self._validate_identifier(manifest.get("account"), "account")
            project = self._validate_identifier(manifest.get("project"), "project")
            account_directory = self._poc_path("accounts", account)
            runtime_directory = self._poc_path("projects", project, "codex-home")
            self._validate_directory(account_directory, 0o700)
            self._validate_directory(runtime_directory, 0o700)
            account_fd = self._open_directory(account_directory, 0o700)
            runtime_fd = self._open_directory(runtime_directory, 0o700)
            try:
                vault_fd = self._open_regular_file_at(account_fd, "auth.json", 0o600)
                runtime_auth_fd = self._open_regular_file_at(
                    runtime_fd, "auth.json", 0o600
                )
                try:
                    vault_metadata = self._metadata_for_fd(vault_fd)
                    runtime_metadata = self._metadata_for_fd(runtime_auth_fd)
                    expected_hash = manifest["vault_before_checkout"]["sha256"]
                    if vault_metadata.sha256 != expected_hash:
                        conflict_name = f"{manifest_id}-runtime-auth.json"
                        self._atomic_copy(
                            runtime_auth_fd,
                            backup_fd,
                            conflict_name,
                            overwrite=False,
                        )
                        manifest["state"] = "credential_conflict"
                        manifest["runtime_at_conflict"] = runtime_metadata.as_dict()
                        manifest["updated_at"] = self._utc_now()
                        self._write_manifest(
                            manifest_fd, manifest_id, manifest, overwrite=True
                        )
                        return {
                            "status": "credential_conflict",
                            "manifest_id": manifest_id,
                        }

                    backup_name = f"{manifest_id}-vault-auth.json"
                    self._atomic_copy(
                        vault_fd, backup_fd, backup_name, overwrite=False
                    )
                    self._atomic_copy(
                        runtime_auth_fd, account_fd, "auth.json", overwrite=True
                    )
                finally:
                    os.close(runtime_auth_fd)
                    os.close(vault_fd)

                committed_fd = self._open_regular_file_at(account_fd, "auth.json", 0o600)
                try:
                    committed_metadata = self._metadata_for_fd(committed_fd)
                finally:
                    os.close(committed_fd)
                if committed_metadata.sha256 != runtime_metadata.sha256:
                    raise HarnessError(
                        "COMMIT_VERIFY_FAILED", "committed credential verification failed"
                    )
                os.unlink("auth.json", dir_fd=runtime_fd)
                os.fsync(runtime_fd)
                manifest["state"] = "committed"
                manifest["vault_after_commit"] = committed_metadata.as_dict()
                manifest["updated_at"] = self._utc_now()
                self._write_manifest(manifest_fd, manifest_id, manifest, overwrite=True)
            finally:
                os.close(runtime_fd)
                os.close(account_fd)
        finally:
            os.close(backup_fd)
            os.close(manifest_fd)
        return {"status": "committed", "manifest_id": manifest_id}

    def compare(self, account: str, project: str) -> dict[str, Any]:
        account = self._validate_identifier(account, "account")
        project = self._validate_identifier(project, "project")
        account_directory = self._poc_path("accounts", account)
        runtime_directory = self._poc_path("projects", project, "codex-home")
        self._validate_directory(account_directory, 0o700)
        self._validate_directory(runtime_directory, 0o700)
        account_fd = self._open_directory(account_directory, 0o700)
        runtime_fd = self._open_directory(runtime_directory, 0o700)
        try:
            vault_fd = self._open_regular_file_at(account_fd, "auth.json", 0o600)
            runtime_auth_fd = self._open_regular_file_at(
                runtime_fd, "auth.json", 0o600
            )
            try:
                before = self._read_json_file(vault_fd)
                after = self._read_json_file(runtime_auth_fd)
            finally:
                os.close(runtime_auth_fd)
                os.close(vault_fd)
        finally:
            os.close(runtime_fd)
            os.close(account_fd)
        changes: list[dict[str, str]] = []
        self._compare_json_values(before, after, "$", changes)
        return {"status": "compared", "changes": changes}

    def _poc_path(self, *parts: str) -> Path:
        candidate = self.poc_root.joinpath(*parts)
        if os.path.commonpath((self.poc_root, candidate)) != os.fspath(self.poc_root):
            raise HarnessError("PATH_REJECTED", "path is outside the POC root")
        return candidate

    def _validate_poc_root_location(self) -> None:
        home = Path.home().absolute()
        global_codex_home = home / ".codex"
        if self.poc_root == Path("/") or self.poc_root == home:
            raise HarnessError("PATH_REJECTED", "POC root path is not allowed")
        if self._is_within(self.poc_root, global_codex_home):
            raise HarnessError("PATH_REJECTED", "POC root path is not allowed")
        if self._is_within(self.poc_root, REPOSITORY_ROOT):
            raise HarnessError("PATH_REJECTED", "POC root path is not allowed")
        if self.poc_root == Path("/mnt") or self._is_within(
            self.poc_root, Path("/mnt")
        ):
            raise HarnessError("PATH_REJECTED", "POC root path is not allowed")

    def _validate_filesystem_type(self, path: Path) -> None:
        best_mount = None
        best_filesystem = None
        try:
            with Path("/proc/self/mountinfo").open("r", encoding="utf-8") as handle:
                for line in handle:
                    fields = line.rstrip("\n").split()
                    separator = fields.index("-")
                    mount_path = Path(self._unescape_mount_field(fields[4]))
                    if not self._is_within(path, mount_path):
                        continue
                    if best_mount is None or len(mount_path.parts) > len(best_mount.parts):
                        best_mount = mount_path
                        best_filesystem = fields[separator + 1]
        except (OSError, ValueError, IndexError) as error:
            raise HarnessError(
                "FILESYSTEM_UNVERIFIED", "unable to verify POC root filesystem"
            ) from error
        if best_filesystem not in APPROVED_FILESYSTEM_TYPES:
            raise HarnessError(
                "FILESYSTEM_REJECTED", "POC root filesystem is not allowed"
            )

    def _unescape_mount_field(self, value: str) -> str:
        return (
            value.replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\012", "\n")
            .replace("\\134", "\\")
        )

    def _is_within(self, child: Path, parent: Path) -> bool:
        try:
            return os.path.commonpath((child, parent)) == os.fspath(parent)
        except ValueError:
            return False

    def _validate_source(self, source_root: Path, source_auth: Path) -> None:
        self._assert_no_symlink_components(source_root)
        self._assert_no_symlink_components(source_auth)
        if os.path.commonpath((source_root, source_auth)) != os.fspath(source_root):
            raise HarnessError("SOURCE_PATH_REJECTED", "source file is outside the approved root")
        self._validate_directory(source_root, None)
        self._validate_directory(source_auth.parent, None)

    def _validate_directory(self, path: Path, expected_mode: int | None) -> None:
        self._assert_no_symlink_components(path)
        try:
            metadata = os.lstat(path)
        except FileNotFoundError as error:
            raise HarnessError("DIRECTORY_MISSING", "required directory is missing") from error
        if not stat.S_ISDIR(metadata.st_mode):
            raise HarnessError("UNSAFE_DIRECTORY", "required path is not a directory")
        if metadata.st_uid != self.current_uid:
            raise HarnessError("OWNER_MISMATCH", "directory owner does not match")
        if expected_mode is not None and stat.S_IMODE(metadata.st_mode) != expected_mode:
            raise HarnessError("MODE_MISMATCH", "directory mode is not allowed")

    def _assert_no_symlink_components(self, path: Path) -> None:
        absolute = Path(os.path.abspath(os.fspath(path)))
        current = Path(absolute.anchor)
        for component in absolute.parts[1:]:
            current /= component
            if current.is_symlink():
                raise HarnessError("SYMLINK_REJECTED", "symbolic links are not allowed")

    def _open_directory(self, path: Path, expected_mode: int) -> int:
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            descriptor = os.open(path, flags)
        except OSError as error:
            raise HarnessError(
                "DIRECTORY_OPEN_FAILED", "unable to open directory safely"
            ) from error
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISDIR(metadata.st_mode)
            or metadata.st_uid != self.current_uid
            or stat.S_IMODE(metadata.st_mode) != expected_mode
        ):
            os.close(descriptor)
            raise HarnessError("UNSAFE_DIRECTORY", "directory validation failed")
        return descriptor

    def _open_source_file(self, path: Path) -> int:
        flags = os.O_RDONLY | os.O_CLOEXEC
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            descriptor = os.open(path, flags)
        except OSError as error:
            raise HarnessError("SOURCE_OPEN_FAILED", "unable to open source file safely") from error
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != self.current_uid:
            os.close(descriptor)
            raise HarnessError("UNSAFE_SOURCE", "source file validation failed")
        return descriptor

    def _open_regular_file_at(
        self, directory_fd: int, name: str, expected_mode: int
    ) -> int:
        flags = os.O_RDONLY | os.O_CLOEXEC
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            descriptor = os.open(name, flags, dir_fd=directory_fd)
        except OSError as error:
            raise HarnessError(
                "FILE_OPEN_FAILED", "unable to open credential file safely"
            ) from error
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != self.current_uid
            or stat.S_IMODE(metadata.st_mode) != expected_mode
        ):
            os.close(descriptor)
            raise HarnessError("UNSAFE_FILE", "credential file validation failed")
        return descriptor

    def _metadata_for_fd(self, descriptor: int) -> FileMetadata:
        metadata = os.fstat(descriptor)
        digest = hashlib.sha256()
        os.lseek(descriptor, 0, os.SEEK_SET)
        while True:
            chunk = os.read(descriptor, BUFFER_SIZE)
            if not chunk:
                break
            digest.update(chunk)
        os.lseek(descriptor, 0, os.SEEK_SET)
        return FileMetadata(
            sha256=digest.hexdigest(),
            size=metadata.st_size,
            mtime_ns=metadata.st_mtime_ns,
            mode=stat.S_IMODE(metadata.st_mode),
            owner_matches=metadata.st_uid == self.current_uid,
        )

    def _read_json_file(self, descriptor: int) -> Any:
        chunks = []
        total = 0
        os.lseek(descriptor, 0, os.SEEK_SET)
        while True:
            chunk = os.read(descriptor, BUFFER_SIZE)
            if not chunk:
                break
            total += len(chunk)
            if total > MAX_JSON_BYTES:
                raise HarnessError("JSON_TOO_LARGE", "credential JSON is too large")
            chunks.append(chunk)
        os.lseek(descriptor, 0, os.SEEK_SET)
        try:
            return json.loads(b"".join(chunks))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise HarnessError("JSON_INVALID", "credential JSON is invalid") from error

    def _compare_json_values(
        self,
        before: Any,
        after: Any,
        path: str,
        changes: list[dict[str, str]],
    ) -> None:
        before_type = self._json_type(before)
        after_type = self._json_type(after)
        if before_type != after_type:
            changes.append(
                {
                    "path": path,
                    "before_type": before_type,
                    "after_type": after_type,
                    "change": "type_changed",
                }
            )
            return
        if isinstance(before, dict):
            before_keys = set(before)
            after_keys = set(after)
            for key in sorted(before_keys - after_keys):
                changes.append(
                    {
                        "path": self._json_child_path(path, key),
                        "before_type": self._json_type(before[key]),
                        "after_type": "missing",
                        "change": "removed",
                    }
                )
            for key in sorted(after_keys - before_keys):
                changes.append(
                    {
                        "path": self._json_child_path(path, key),
                        "before_type": "missing",
                        "after_type": self._json_type(after[key]),
                        "change": "added",
                    }
                )
            for key in sorted(before_keys & after_keys):
                self._compare_json_values(
                    before[key],
                    after[key],
                    self._json_child_path(path, key),
                    changes,
                )
            return
        if isinstance(before, list):
            shared_length = min(len(before), len(after))
            for index in range(shared_length):
                self._compare_json_values(
                    before[index], after[index], f"{path}[{index}]", changes
                )
            for index in range(shared_length, len(before)):
                changes.append(
                    {
                        "path": f"{path}[{index}]",
                        "before_type": self._json_type(before[index]),
                        "after_type": "missing",
                        "change": "removed",
                    }
                )
            for index in range(shared_length, len(after)):
                changes.append(
                    {
                        "path": f"{path}[{index}]",
                        "before_type": "missing",
                        "after_type": self._json_type(after[index]),
                        "change": "added",
                    }
                )
            return
        if before != after:
            changes.append(
                {
                    "path": path,
                    "before_type": before_type,
                    "after_type": after_type,
                    "change": "changed",
                }
            )

    def _json_child_path(self, parent: str, key: str) -> str:
        key_fingerprint = hashlib.sha256(key.encode("utf-8")).hexdigest()[:12]
        return f'{parent}["key:{key_fingerprint}"]'

    def _json_type(self, value: Any) -> str:
        if value is None:
            return "null"
        if isinstance(value, bool):
            return "boolean"
        if isinstance(value, dict):
            return "object"
        if isinstance(value, list):
            return "array"
        if isinstance(value, str):
            return "string"
        if isinstance(value, (int, float)):
            return "number"
        return "unknown"

    def _utc_now(self) -> str:
        return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace(
            "+00:00", "Z"
        )

    def _atomic_copy(
        self,
        source_fd: int,
        destination_directory_fd: int,
        destination_name: str,
        *,
        overwrite: bool,
    ) -> None:
        temporary_name = f".credential-{secrets.token_hex(12)}.tmp"
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            temporary_fd = os.open(
                temporary_name,
                flags,
                0o600,
                dir_fd=destination_directory_fd,
            )
        except OSError as error:
            raise HarnessError("TEMP_CREATE_FAILED", "unable to create temporary file") from error
        try:
            os.fchmod(temporary_fd, 0o600)
            os.lseek(source_fd, 0, os.SEEK_SET)
            while True:
                chunk = os.read(source_fd, BUFFER_SIZE)
                if not chunk:
                    break
                view = memoryview(chunk)
                while view:
                    written = os.write(temporary_fd, view)
                    view = view[written:]
            os.fsync(temporary_fd)
        except OSError as error:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise HarnessError("TEMP_WRITE_FAILED", "unable to persist temporary file") from error
        finally:
            os.close(temporary_fd)

        try:
            if not overwrite:
                self._assert_missing_at(
                    destination_directory_fd,
                    destination_name,
                    "DESTINATION_EXISTS",
                )
            os.replace(
                temporary_name,
                destination_name,
                src_dir_fd=destination_directory_fd,
                dst_dir_fd=destination_directory_fd,
            )
            os.fsync(destination_directory_fd)
        except HarnessError:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise
        except OSError as error:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise HarnessError("ATOMIC_RENAME_FAILED", "atomic replacement failed") from error

    def _assert_missing_at(self, directory_fd: int, name: str, code: str) -> None:
        try:
            os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
        except FileNotFoundError:
            return
        raise HarnessError(code, "destination already exists")

    def _unlink_if_exists(self, directory_fd: int, name: str) -> None:
        try:
            os.unlink(name, dir_fd=directory_fd)
        except FileNotFoundError:
            return

    def _write_manifest(
        self,
        manifest_directory_fd: int,
        manifest_id: str,
        manifest: dict[str, Any],
        *,
        overwrite: bool,
    ) -> None:
        payload = json.dumps(
            manifest, sort_keys=True, separators=(",", ":")
        ).encode("utf-8")
        self._atomic_write_bytes(
            payload,
            manifest_directory_fd,
            f"{manifest_id}.json",
            overwrite=overwrite,
        )

    def _atomic_write_bytes(
        self,
        payload: bytes,
        destination_directory_fd: int,
        destination_name: str,
        *,
        overwrite: bool,
    ) -> None:
        temporary_name = f".manifest-{secrets.token_hex(12)}.tmp"
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            temporary_fd = os.open(
                temporary_name,
                flags,
                0o600,
                dir_fd=destination_directory_fd,
            )
        except OSError as error:
            raise HarnessError("TEMP_CREATE_FAILED", "unable to create temporary file") from error
        try:
            os.fchmod(temporary_fd, 0o600)
            view = memoryview(payload)
            while view:
                written = os.write(temporary_fd, view)
                view = view[written:]
            os.fsync(temporary_fd)
        except OSError as error:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise HarnessError("TEMP_WRITE_FAILED", "unable to persist temporary file") from error
        finally:
            os.close(temporary_fd)
        try:
            if not overwrite:
                self._assert_missing_at(
                    destination_directory_fd,
                    destination_name,
                    "DESTINATION_EXISTS",
                )
            os.replace(
                temporary_name,
                destination_name,
                src_dir_fd=destination_directory_fd,
                dst_dir_fd=destination_directory_fd,
            )
            os.fsync(destination_directory_fd)
        except HarnessError:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise
        except OSError as error:
            self._unlink_if_exists(destination_directory_fd, temporary_name)
            raise HarnessError("ATOMIC_RENAME_FAILED", "atomic replacement failed") from error

    def _read_manifest(self, manifest_directory_fd: int, manifest_id: str) -> dict[str, Any]:
        descriptor = self._open_regular_file_at(
            manifest_directory_fd, f"{manifest_id}.json", 0o600
        )
        try:
            chunks = []
            while True:
                chunk = os.read(descriptor, BUFFER_SIZE)
                if not chunk:
                    break
                chunks.append(chunk)
            try:
                value = json.loads(b"".join(chunks))
            except json.JSONDecodeError as error:
                raise HarnessError("MANIFEST_INVALID", "manifest is invalid") from error
        finally:
            os.close(descriptor)
        if not isinstance(value, dict):
            raise HarnessError("MANIFEST_INVALID", "manifest is invalid")
        return value

    def _validate_identifier(self, value: Any, kind: str) -> str:
        if not isinstance(value, str) or not IDENTIFIER_PATTERN.fullmatch(value):
            raise HarnessError("IDENTIFIER_REJECTED", f"{kind} identifier is invalid")
        return value

    def _validate_manifest_id(self, value: str) -> str:
        if not isinstance(value, str) or not re.fullmatch(
            r"checkout-[0-9a-f]{24}", value
        ):
            raise HarnessError("MANIFEST_ID_REJECTED", "manifest identifier is invalid")
        return value


class HarnessArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise HarnessError("INVALID_ARGUMENTS", "command arguments are invalid")


def build_argument_parser() -> HarnessArgumentParser:
    parser = HarnessArgumentParser(description=__doc__)
    parser.add_argument("--poc-root", required=True, type=Path)
    subparsers = parser.add_subparsers(dest="command", required=True)

    import_parser = subparsers.add_parser("import")
    import_parser.add_argument("--source-root", required=True, type=Path)
    import_parser.add_argument("--source-auth", required=True, type=Path)
    import_parser.add_argument("--account", required=True)

    checkout_parser = subparsers.add_parser("checkout")
    checkout_parser.add_argument("--account", required=True)
    checkout_parser.add_argument("--project", required=True)

    commit_parser = subparsers.add_parser("commit")
    commit_parser.add_argument("--manifest-id", required=True)

    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--account", required=True)
    compare_parser.add_argument("--project", required=True)
    return parser


def main(arguments: list[str] | None = None) -> int:
    try:
        parsed = build_argument_parser().parse_args(arguments)
        harness = CredentialHarness(parsed.poc_root)
        if parsed.command == "import":
            result = harness.import_credential(
                parsed.source_root, parsed.source_auth, parsed.account
            )
        elif parsed.command == "checkout":
            result = harness.checkout(parsed.account, parsed.project)
        elif parsed.command == "commit":
            result = harness.commit(parsed.manifest_id)
        elif parsed.command == "compare":
            result = harness.compare(parsed.account, parsed.project)
        else:
            raise HarnessError("INVALID_ARGUMENTS", "command is invalid")
    except HarnessError as error:
        print(
            json.dumps(
                {"status": "error", "error_code": error.code},
                sort_keys=True,
                separators=(",", ":"),
            )
        )
        return 2
    except Exception:
        print('{"error_code":"INTERNAL_ERROR","status":"error"}')
        return 2
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    if result.get("status") == "credential_conflict":
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
