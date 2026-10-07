CREATE TABLE provider_edit_tokens (
    config_id TEXT PRIMARY KEY REFERENCES ai_provider_configs(id) ON DELETE CASCADE,
    token TEXT NOT NULL
);
INSERT INTO provider_edit_tokens(config_id,token)
SELECT id,lower(hex(randomblob(16))) FROM ai_provider_configs;
CREATE TRIGGER provider_edit_token_insert AFTER INSERT ON ai_provider_configs
BEGIN INSERT INTO provider_edit_tokens(config_id,token) VALUES(NEW.id,lower(hex(randomblob(16)))); END;
CREATE TRIGGER provider_edit_token_update AFTER UPDATE OF name,provider_type,base_url,model,api_key,temperature,max_tokens,context_window ON ai_provider_configs
BEGIN UPDATE provider_edit_tokens SET token=lower(hex(randomblob(16))) WHERE config_id=NEW.id; END;
CREATE TRIGGER provider_edit_token_key_insert AFTER INSERT ON provider_key_secrets
BEGIN UPDATE provider_edit_tokens SET token=lower(hex(randomblob(16))) WHERE config_id=NEW.config_id; END;
CREATE TRIGGER provider_edit_token_key_update AFTER UPDATE ON provider_key_secrets
BEGIN UPDATE provider_edit_tokens SET token=lower(hex(randomblob(16))) WHERE config_id=NEW.config_id; END;
CREATE TRIGGER provider_edit_token_key_delete AFTER DELETE ON provider_key_secrets
BEGIN UPDATE provider_edit_tokens SET token=lower(hex(randomblob(16))) WHERE config_id=OLD.config_id; END;
