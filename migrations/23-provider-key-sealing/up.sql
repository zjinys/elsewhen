CREATE TABLE provider_key_secrets (
    config_id TEXT PRIMARY KEY REFERENCES ai_provider_configs(id) ON DELETE CASCADE,
    ciphertext BLOB NOT NULL
);
CREATE TABLE provider_key_policy (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1)
);
CREATE TRIGGER provider_key_policy_requires_sealed_keys BEFORE INSERT ON provider_key_policy
WHEN EXISTS(SELECT 1 FROM ai_provider_configs WHERE api_key IS NOT NULL)
BEGIN SELECT RAISE(ABORT,'provider_keys_not_sealed'); END;
CREATE TRIGGER provider_key_policy_no_delete BEFORE DELETE ON provider_key_policy
BEGIN SELECT RAISE(ABORT,'provider_key_policy_immutable'); END;
CREATE TRIGGER provider_key_policy_no_update BEFORE UPDATE ON provider_key_policy
BEGIN SELECT RAISE(ABORT,'provider_key_policy_immutable'); END;
CREATE TRIGGER provider_key_no_plaintext_insert BEFORE INSERT ON ai_provider_configs
WHEN NEW.api_key IS NOT NULL AND EXISTS(SELECT 1 FROM provider_key_policy)
BEGIN SELECT RAISE(ABORT,'provider_key_must_be_sealed'); END;
CREATE TRIGGER provider_key_no_plaintext_update BEFORE UPDATE OF api_key ON ai_provider_configs
WHEN NEW.api_key IS NOT NULL AND EXISTS(SELECT 1 FROM provider_key_policy)
BEGIN SELECT RAISE(ABORT,'provider_key_must_be_sealed'); END;
