import importlib.util
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


MODULE_PATH = Path(__file__).parents[1] / "credential_harness.py"


def load_harness_module():
    spec = importlib.util.spec_from_file_location("credential_harness", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class CredentialHarnessTest(unittest.TestCase):
    def setUp(self):
        self.temp_directory = tempfile.TemporaryDirectory(prefix="muxlane phase 2 ")
        self.base = Path(self.temp_directory.name)
        self.source_root = self.base / "approved source"
        self.source_account = self.source_root / "source account"
        self.source_auth = self.source_account / "auth.json"
        self.poc_root = self.base / "poc root"

        self.source_account.mkdir(parents=True, mode=0o755)
        self.source_auth.write_text(
            json.dumps({"synthetic": "credential-material"}), encoding="utf-8"
        )
        self.source_auth.chmod(0o644)

        required_directories = (
            "accounts/account-a",
            "accounts/account-b",
            "projects/project-a/codex-home",
            "projects/project-b/codex-home",
            "backups",
            "evidence",
            "manifests",
            "tmp",
        )
        self.poc_root.mkdir(mode=0o700)
        for relative_path in required_directories:
            directory = self.poc_root / relative_path
            directory.mkdir(parents=True, exist_ok=True)
            directory.chmod(0o700)
        for directory in (
            self.poc_root / "accounts",
            self.poc_root / "projects",
            self.poc_root / "projects/project-a",
            self.poc_root / "projects/project-b",
        ):
            directory.chmod(0o700)

        self.harness_module = load_harness_module()
        self.harness = self.harness_module.CredentialHarness(self.poc_root)

    def tearDown(self):
        self.temp_directory.cleanup()

    def import_account(self):
        return self.harness.import_credential(
            self.source_root, self.source_auth, "account-a"
        )

    def assert_harness_error(self, code, callback):
        with self.assertRaises(self.harness_module.HarnessError) as context:
            callback()
        self.assertEqual(context.exception.code, code)
        return context.exception

    def run_cli(self, *arguments):
        return subprocess.run(
            [
                sys.executable,
                os.fspath(MODULE_PATH),
                "--poc-root",
                os.fspath(self.poc_root),
                *arguments,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_import_checkout_commit_preserves_source_and_cleans_runtime(self):
        source_before = self.source_auth.read_bytes()

        import_result = self.harness.import_credential(
            self.source_root, self.source_auth, "account-a"
        )
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        self.assertEqual(import_result["status"], "imported")
        self.assertEqual(stat.S_IMODE(vault_auth.stat().st_mode), 0o600)

        checkout_result = self.harness.checkout("account-a", "project-a")
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        self.assertEqual(checkout_result["status"], "checked_out")
        self.assertEqual(stat.S_IMODE(runtime_auth.stat().st_mode), 0o600)
        self.assertEqual(runtime_auth.read_bytes(), vault_auth.read_bytes())

        runtime_auth.write_text(
            json.dumps({"synthetic": "credential-material-updated"}), encoding="utf-8"
        )
        runtime_auth.chmod(0o600)
        commit_result = self.harness.commit(checkout_result["manifest_id"])

        self.assertEqual(commit_result["status"], "committed")
        self.assertFalse(runtime_auth.exists())
        self.assertIn(b"updated", vault_auth.read_bytes())
        self.assertEqual(self.source_auth.read_bytes(), source_before)
        self.assertEqual(stat.S_IMODE(self.source_auth.stat().st_mode), 0o644)

    def test_manifest_records_utc_creation_and_update_times(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        manifest_path = (
            self.poc_root / "manifests" / f"{checkout['manifest_id']}.json"
        )
        checked_out = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertRegex(checked_out["created_at"], r"^\d{4}-\d{2}-\d{2}T.*Z$")
        self.assertRegex(checked_out["updated_at"], r"^\d{4}-\d{2}-\d{2}T.*Z$")

        self.harness.commit(checkout["manifest_id"])
        committed = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(committed["created_at"], checked_out["created_at"])
        self.assertRegex(committed["updated_at"], r"^\d{4}-\d{2}-\d{2}T.*Z$")

    def test_checkout_rejects_missing_vault(self):
        self.assert_harness_error(
            "FILE_OPEN_FAILED",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_checkout_rejects_vault_mode_other_than_0600(self):
        self.import_account()
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_auth.chmod(0o644)
        self.assert_harness_error(
            "UNSAFE_FILE",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_checkout_rejects_vault_with_wrong_owner(self):
        self.import_account()
        original_fstat = self.harness_module.os.fstat

        def mismatched_file_owner(descriptor):
            metadata = original_fstat(descriptor)
            if stat.S_ISREG(metadata.st_mode):
                return SimpleNamespace(
                    st_mode=metadata.st_mode,
                    st_uid=metadata.st_uid + 1,
                )
            return metadata

        with mock.patch.object(
            self.harness_module.os, "fstat", side_effect=mismatched_file_owner
        ):
            self.assert_harness_error(
                "UNSAFE_FILE",
                lambda: self.harness.checkout("account-a", "project-a"),
            )

    def test_checkout_rejects_vault_symlink(self):
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_auth.symlink_to(self.source_auth)
        self.assert_harness_error(
            "FILE_OPEN_FAILED",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_checkout_rejects_existing_runtime_auth_and_repeated_checkout(self):
        self.import_account()
        first = self.harness.checkout("account-a", "project-a")
        self.assertEqual(first["status"], "checked_out")
        self.assert_harness_error(
            "RUNTIME_AUTH_EXISTS",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_checkout_rejects_existing_runtime_symlink(self):
        self.import_account()
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.symlink_to(self.source_auth)
        self.assert_harness_error(
            "RUNTIME_AUTH_EXISTS",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_import_rejects_source_symlink(self):
        source_link = self.source_account / "linked-auth.json"
        source_link.symlink_to(self.source_auth)
        self.assert_harness_error(
            "SYMLINK_REJECTED",
            lambda: self.harness.import_credential(
                self.source_root, source_link, "account-a"
            ),
        )

    def test_harness_rejects_symlink_parent(self):
        linked_root = self.base / "linked-poc"
        linked_root.symlink_to(self.poc_root, target_is_directory=True)
        self.assert_harness_error(
            "SYMLINK_REJECTED",
            lambda: self.harness_module.CredentialHarness(linked_root),
        )

    def test_import_rejects_similar_source_prefix(self):
        outside_root = self.base / "approved source-other"
        outside_root.mkdir(mode=0o755)
        outside_auth = outside_root / "auth.json"
        outside_auth.write_text('{"synthetic":"outside"}', encoding="utf-8")
        outside_auth.chmod(0o644)
        self.assert_harness_error(
            "SOURCE_PATH_REJECTED",
            lambda: self.harness.import_credential(
                self.source_root, outside_auth, "account-a"
            ),
        )

    def test_import_reports_temporary_file_creation_failure(self):
        temporary_name = ".credential-" + "f" * 24 + ".tmp"
        blocking_file = self.poc_root / "accounts/account-a" / temporary_name
        blocking_file.write_text("occupied", encoding="utf-8")
        blocking_file.chmod(0o600)
        with mock.patch.object(
            self.harness_module.secrets, "token_hex", return_value="f" * 24
        ):
            self.assert_harness_error("TEMP_CREATE_FAILED", self.import_account)

    def test_import_removes_partial_temporary_file_after_write_failure(self):
        with mock.patch.object(
            self.harness_module.os,
            "write",
            side_effect=OSError("synthetic write failure"),
        ):
            self.assert_harness_error("TEMP_WRITE_FAILED", self.import_account)
        temporary_files = list(
            (self.poc_root / "accounts/account-a").glob(".credential-*.tmp")
        )
        self.assertEqual(temporary_files, [])

    def test_checkout_rejects_runtime_directory_with_unsafe_mode(self):
        self.import_account()
        runtime_directory = self.poc_root / "projects/project-a/codex-home"
        runtime_directory.chmod(0o500)
        self.assert_harness_error(
            "MODE_MISMATCH",
            lambda: self.harness.checkout("account-a", "project-a"),
        )

    def test_import_rejects_vault_directory_with_unsafe_mode(self):
        vault_directory = self.poc_root / "accounts/account-a"
        vault_directory.chmod(0o500)
        self.assert_harness_error("MODE_MISMATCH", self.import_account)

    def test_commit_detects_hash_conflict_without_overwriting_vault(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        vault_auth.write_text('{"synthetic":"newer-vault"}', encoding="utf-8")
        vault_auth.chmod(0o600)
        runtime_auth.write_text('{"synthetic":"runtime-update"}', encoding="utf-8")
        runtime_auth.chmod(0o600)
        vault_before_commit = vault_auth.read_bytes()

        result = self.harness.commit(checkout["manifest_id"])

        self.assertEqual(result["status"], "credential_conflict")
        self.assertEqual(vault_auth.read_bytes(), vault_before_commit)
        self.assertTrue(runtime_auth.exists())
        backups = list(
            (self.poc_root / "backups").glob("*-runtime-auth.json")
        )
        self.assertEqual(len(backups), 1)
        self.assertEqual(stat.S_IMODE(backups[0].stat().st_mode), 0o600)
        self.assertEqual(backups[0].read_bytes(), runtime_auth.read_bytes())

    def test_commit_rejects_runtime_mode_0644_and_preserves_runtime(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.chmod(0o644)
        self.assert_harness_error(
            "UNSAFE_FILE",
            lambda: self.harness.commit(checkout["manifest_id"]),
        )
        self.assertTrue(runtime_auth.exists())

    def test_commit_rejects_runtime_symlink_and_preserves_vault(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_before = vault_auth.read_bytes()
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.unlink()
        runtime_auth.symlink_to(self.source_auth)
        self.assert_harness_error(
            "FILE_OPEN_FAILED",
            lambda: self.harness.commit(checkout["manifest_id"]),
        )
        self.assertEqual(vault_auth.read_bytes(), vault_before)
        self.assertTrue(runtime_auth.is_symlink())

    def test_commit_rejects_vault_symlink_and_preserves_runtime(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_auth.unlink()
        vault_auth.symlink_to(self.source_auth)
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        self.assert_harness_error(
            "FILE_OPEN_FAILED",
            lambda: self.harness.commit(checkout["manifest_id"]),
        )
        self.assertTrue(runtime_auth.exists())

    def test_repeated_commit_is_rejected_without_changing_vault(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        first = self.harness.commit(checkout["manifest_id"])
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_after_first = vault_auth.read_bytes()
        self.assertEqual(first["status"], "committed")
        self.assert_harness_error(
            "INVALID_MANIFEST_STATE",
            lambda: self.harness.commit(checkout["manifest_id"]),
        )
        self.assertEqual(vault_auth.read_bytes(), vault_after_first)

    def test_error_does_not_include_file_contents(self):
        secret_marker = "UNIQUE_SYNTHETIC_SECRET_MARKER"
        self.source_auth.write_text(secret_marker, encoding="utf-8")
        self.source_auth.chmod(0o644)
        self.import_account()
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.write_text("occupied", encoding="utf-8")
        runtime_auth.chmod(0o600)
        error = self.assert_harness_error(
            "RUNTIME_AUTH_EXISTS",
            lambda: self.harness.checkout("account-a", "project-a"),
        )
        self.assertNotIn(secret_marker, str(error))

    def test_harness_rejects_relative_poc_root(self):
        self.assert_harness_error(
            "PATH_REJECTED",
            lambda: self.harness_module.CredentialHarness(Path("relative-poc-root")),
        )

    def test_harness_rejects_repository_as_poc_root(self):
        repository_root = MODULE_PATH.parents[2]
        self.assert_harness_error(
            "PATH_REJECTED",
            lambda: self.harness_module.CredentialHarness(repository_root),
        )

    def test_harness_rejects_global_codex_home_as_poc_root(self):
        self.assert_harness_error(
            "PATH_REJECTED",
            lambda: self.harness_module.CredentialHarness(Path.home() / ".codex"),
        )

    def test_harness_rejects_windows_mount_as_poc_root(self):
        self.assert_harness_error(
            "PATH_REJECTED",
            lambda: self.harness_module.CredentialHarness(
                Path("/mnt/c/muxlane-phase-2-runtime")
            ),
        )

    def test_cli_propagates_failure_exit_code_without_secret_output(self):
        secret_marker = "CLI_MUST_NOT_PRINT_THIS_VALUE"
        self.source_auth.write_text(secret_marker, encoding="utf-8")
        self.source_auth.chmod(0o644)

        result = self.run_cli(
            "checkout", "--account", "account-a", "--project", "project-a"
        )

        self.assertEqual(result.returncode, 2)
        response = json.loads(result.stdout)
        self.assertEqual(response["status"], "error")
        self.assertEqual(response["error_code"], "FILE_OPEN_FAILED")
        self.assertNotIn(secret_marker, result.stdout)
        self.assertNotIn(secret_marker, result.stderr)

    def test_cli_uses_distinct_exit_code_for_credential_conflict(self):
        self.import_account()
        checkout = self.harness.checkout("account-a", "project-a")
        vault_auth = self.poc_root / "accounts/account-a/auth.json"
        vault_auth.write_text('{"synthetic":"newer-vault"}', encoding="utf-8")
        vault_auth.chmod(0o600)

        result = self.run_cli(
            "commit", "--manifest-id", checkout["manifest_id"]
        )

        self.assertEqual(result.returncode, 3)
        response = json.loads(result.stdout)
        self.assertEqual(response["status"], "credential_conflict")

    def test_compare_reports_only_paths_types_and_change_kinds(self):
        self.import_account()
        self.harness.checkout("account-a", "project-a")
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.write_text(
            json.dumps(
                {
                    "synthetic": "runtime-secret-value",
                    "added": {"nested": 7},
                }
            ),
            encoding="utf-8",
        )
        runtime_auth.chmod(0o600)

        result = self.harness.compare("account-a", "project-a")
        serialized = json.dumps(result, sort_keys=True)
        synthetic_path = self.harness._json_child_path("$", "synthetic")
        added_path = self.harness._json_child_path("$", "added")

        self.assertEqual(result["status"], "compared")
        self.assertIn(
            {
                "path": synthetic_path,
                "before_type": "string",
                "after_type": "string",
                "change": "changed",
            },
            result["changes"],
        )
        self.assertIn(
            {
                "path": added_path,
                "before_type": "missing",
                "after_type": "object",
                "change": "added",
            },
            result["changes"],
        )
        self.assertNotIn("credential-material", serialized)
        self.assertNotIn("runtime-secret-value", serialized)

    def test_compare_hashes_object_keys_that_contain_identity_material(self):
        self.source_auth.write_text(
            json.dumps(
                {
                    "accounts": {
                        "source@example.invalid": {"token": "SECRET_VALUE_ALPHA"}
                    }
                }
            ),
            encoding="utf-8",
        )
        self.source_auth.chmod(0o644)
        self.import_account()
        self.harness.checkout("account-a", "project-a")
        runtime_auth = self.poc_root / "projects/project-a/codex-home/auth.json"
        runtime_auth.write_text(
            json.dumps(
                {
                    "accounts": {
                        "source@example.invalid": {"token": "SECRET_VALUE_OMEGA"}
                    }
                }
            ),
            encoding="utf-8",
        )
        runtime_auth.chmod(0o600)

        result = self.harness.compare("account-a", "project-a")
        serialized = json.dumps(result, sort_keys=True)

        self.assertEqual(result["status"], "compared")
        self.assertNotIn("source@example.invalid", serialized)
        self.assertNotIn("accounts", serialized)
        self.assertNotIn("token", serialized)
        self.assertNotIn("SECRET_VALUE_ALPHA", serialized)
        self.assertNotIn("SECRET_VALUE_OMEGA", serialized)


if __name__ == "__main__":
    unittest.main()
