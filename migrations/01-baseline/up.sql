CREATE TABLE schema_migrations (
               version INTEGER PRIMARY KEY,
               applied_at TEXT NOT NULL
             );
CREATE TABLE events (
               id TEXT PRIMARY KEY,
               occurred_at TEXT NOT NULL,
               recorded_at TEXT NOT NULL,
               processed_at TEXT,
               raw_text TEXT NOT NULL CHECK (length(trim(raw_text)) > 0),
               source TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
CREATE INDEX idx_events_recorded_at ON events(recorded_at);
CREATE INDEX idx_events_status ON events(status);
CREATE TRIGGER prevent_raw_event_mutation
             BEFORE UPDATE OF raw_text, recorded_at, source ON events
             BEGIN
               SELECT RAISE(ABORT, 'raw_event_is_immutable');
             END;
CREATE TABLE analysis_jobs (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
               attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT, available_at TEXT NOT NULL,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id), UNIQUE(event_id)
             );
CREATE INDEX idx_analysis_jobs_ready ON analysis_jobs(status, available_at);
CREATE TABLE event_analyses (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, prompt_version TEXT NOT NULL,
               result_json TEXT NOT NULL, created_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
CREATE TABLE ai_provider_configs (
               id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
               provider_type TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL,
               api_key_source TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             , api_key TEXT, is_active INTEGER NOT NULL DEFAULT 0, temperature REAL NOT NULL DEFAULT 0.7, max_tokens INTEGER);
CREATE TABLE conversations (
               id TEXT PRIMARY KEY,
               title TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             , tag TEXT CHECK(tag IN ('diary', 'idea', 'discussion', 'general')), archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)), wiki_page_slug TEXT, assistant_mode TEXT NOT NULL DEFAULT 'personal_secretary');
CREATE TABLE messages (
               id TEXT PRIMARY KEY,
               conversation_id TEXT NOT NULL,
               role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
               content TEXT NOT NULL,
               created_at TEXT NOT NULL, parent_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
               FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
             );
CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at);
CREATE INDEX idx_messages_parent ON messages(parent_message_id);
CREATE TABLE insights (
               id TEXT PRIMARY KEY,
               created_at TEXT NOT NULL,
               window_days INTEGER NOT NULL,
               prompt_version TEXT NOT NULL,
               lens TEXT NOT NULL,
               title TEXT NOT NULL,
               observation TEXT NOT NULL,
               related_raw TEXT NOT NULL DEFAULT '[]',
               action TEXT,
               status TEXT NOT NULL DEFAULT 'new'
             );
CREATE INDEX idx_insights_created_at ON insights(created_at);
CREATE TABLE app_meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );
CREATE TABLE wiki_pages (
               id TEXT PRIMARY KEY,
               slug TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               title TEXT NOT NULL,
               summary TEXT NOT NULL DEFAULT '',
               content_md TEXT NOT NULL,
               tags TEXT NOT NULL DEFAULT '[]',
               source_event_ids TEXT NOT NULL DEFAULT '[]',
               evidence_count INTEGER NOT NULL DEFAULT 1,
               first_seen_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'active',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             , source_url TEXT, area TEXT, based_on TEXT, content_type TEXT, human_edited_at TEXT, opinion TEXT);
CREATE INDEX idx_wiki_pages_kind ON wiki_pages(kind);
CREATE TABLE wiki_revisions (
               id TEXT PRIMARY KEY,
               page_id TEXT NOT NULL,
               content_md TEXT NOT NULL,
               reason TEXT NOT NULL,
               source_event_id TEXT,
               created_at TEXT NOT NULL,
               FOREIGN KEY(page_id) REFERENCES wiki_pages(id) ON DELETE CASCADE
             );
CREATE INDEX idx_wiki_revisions_page ON wiki_revisions(page_id, created_at);
CREATE TABLE wiki_log (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               ts TEXT NOT NULL,
               entry TEXT NOT NULL
             );
CREATE TABLE token_usage (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                model TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
              );
CREATE INDEX idx_token_usage_created ON token_usage(created_at);
CREATE TABLE rules (
               id TEXT PRIMARY KEY,
               content TEXT NOT NULL CHECK (length(trim(content)) > 0),
               status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','pending')),
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             , conversation_id TEXT);
CREATE TABLE pending_actions (
               id TEXT PRIMARY KEY,
               conversation_id TEXT NOT NULL,
               action TEXT NOT NULL,
               args_json TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','done','declined')),
               created_at TEXT NOT NULL,
               FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
             );
CREATE INDEX idx_pending_actions_conv
               ON pending_actions(conversation_id, status, created_at);
CREATE UNIQUE INDEX idx_ai_provider_configs_single_active
                       ON ai_provider_configs(is_active) WHERE is_active=1;
CREATE TABLE todos (
               id TEXT PRIMARY KEY,
               title TEXT NOT NULL CHECK(length(trim(title)) > 0),
               status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','done','archived')),
               priority TEXT NOT NULL DEFAULT 'normal' CHECK(priority IN ('high','normal','low')),
               due_at TEXT,
               related_event_id TEXT,
               related_wiki_slug TEXT,
               note TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
CREATE INDEX idx_todos_status ON todos(status, created_at);
CREATE TABLE relations (
               id TEXT PRIMARY KEY,
               from_slug TEXT NOT NULL,
               from_kind TEXT NOT NULL,
               to_slug TEXT NOT NULL,
               to_kind TEXT NOT NULL,
               relation TEXT NOT NULL,
               note TEXT,
               confidence INTEGER NOT NULL DEFAULT 3,
               source_conversation_id TEXT,
               created_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL, source_event_id TEXT,
               UNIQUE(from_slug, to_slug, relation)
             );
CREATE INDEX idx_relations_from ON relations(from_slug);
CREATE INDEX idx_relations_to ON relations(to_slug);
CREATE TABLE input_records (
               id TEXT PRIMARY KEY,
               raw_text TEXT NOT NULL CHECK(length(trim(raw_text)) > 0),
               source TEXT NOT NULL,
               route_status TEXT NOT NULL DEFAULT 'pending'
                 CHECK(route_status IN ('pending','routed','needs_confirmation','failed')),
               idempotency_key TEXT,
               event_id TEXT,
               message_id TEXT,
               wiki_page_slug TEXT,
               todo_id TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id),
               FOREIGN KEY(message_id) REFERENCES messages(id),
               FOREIGN KEY(todo_id) REFERENCES todos(id)
             );
CREATE UNIQUE INDEX idx_input_records_idempotency
               ON input_records(idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_input_records_created_at
               ON input_records(created_at);
CREATE TABLE daily_reviews (
               id TEXT PRIMARY KEY,
               review_date TEXT NOT NULL,
               prompt_version TEXT NOT NULL,
               result_json TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
CREATE INDEX idx_daily_reviews_date
               ON daily_reviews(review_date, created_at);
CREATE TABLE daily_review_sources (
               review_id TEXT NOT NULL,
               event_id TEXT NOT NULL,
               PRIMARY KEY(review_id, event_id),
               FOREIGN KEY(review_id) REFERENCES daily_reviews(id) ON DELETE CASCADE,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
CREATE TABLE entity_facts (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL CHECK(entity_kind IN ('person','project','topic')),
               entity_slug TEXT NOT NULL,
               fact_text TEXT NOT NULL CHECK(length(trim(fact_text)) > 0),
               occurred_at TEXT NOT NULL,
               confidence INTEGER NOT NULL CHECK(confidence BETWEEN 0 AND 5),
               source_event_id TEXT NOT NULL,
               created_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               FOREIGN KEY(source_event_id) REFERENCES events(id),
               UNIQUE(entity_kind, entity_slug, fact_text, source_event_id)
             );
CREATE INDEX idx_entity_facts_entity
               ON entity_facts(entity_kind, entity_slug, occurred_at DESC);
CREATE INDEX idx_entity_facts_source
               ON entity_facts(source_event_id);
CREATE TABLE entity_aliases (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL CHECK(entity_kind IN ('person','project','topic')),
               entity_slug TEXT NOT NULL,
               alias TEXT NOT NULL CHECK(length(trim(alias)) > 0),
               created_at TEXT NOT NULL,
               UNIQUE(entity_kind, entity_slug, alias)
             );
CREATE INDEX idx_entity_aliases_lookup ON entity_aliases(alias);
CREATE TABLE entity_merges (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL,
               source_slug TEXT NOT NULL,
               target_slug TEXT NOT NULL,
               created_at TEXT NOT NULL,
               undone_at TEXT
             );
CREATE TABLE entity_merge_snapshots (
               merge_id TEXT NOT NULL,
               table_name TEXT NOT NULL,
               row_id TEXT NOT NULL,
               disposition TEXT NOT NULL DEFAULT 'moved' CHECK(disposition IN ('moved','deduplicated')),
               payload TEXT NOT NULL,
               PRIMARY KEY(merge_id, table_name, row_id),
               FOREIGN KEY(merge_id) REFERENCES entity_merges(id) ON DELETE CASCADE
             );
CREATE TABLE event_recordability_decisions (
               id TEXT PRIMARY KEY,
               event_id TEXT NOT NULL,
               recordable INTEGER NOT NULL CHECK(recordable IN (0,1)),
               kind TEXT NOT NULL CHECK(kind IN ('event','discussion','chitchat','meta')),
               reason TEXT NOT NULL,
               created_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
CREATE INDEX idx_event_recordability_latest
               ON event_recordability_decisions(event_id, created_at DESC, id DESC);
CREATE TABLE knowledge_digest_jobs (
           id TEXT PRIMARY KEY,
           event_id TEXT NOT NULL,
           digest_version TEXT NOT NULL,
           status TEXT NOT NULL
             CHECK(status IN ('pending','running','retry','succeeded','failed','skipped')),
           attempts INTEGER NOT NULL DEFAULT 0,
           failed_rounds INTEGER NOT NULL DEFAULT 0,
           last_error TEXT,
           skip_reason TEXT,
           batch_id TEXT,
           available_at TEXT NOT NULL,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           FOREIGN KEY(event_id) REFERENCES events(id),
           UNIQUE(event_id, digest_version)
         );
CREATE INDEX idx_knowledge_digest_jobs_ready
           ON knowledge_digest_jobs(digest_version, status, available_at);
CREATE INDEX idx_knowledge_digest_jobs_batch
           ON knowledge_digest_jobs(batch_id);
CREATE TABLE knowledge_digest_runs (
           id TEXT PRIMARY KEY,
           started_at TEXT NOT NULL,
           finished_at TEXT,
           status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed')),
           event_count INTEGER NOT NULL,
           model TEXT,
           duration_ms INTEGER,
           created_slugs TEXT NOT NULL DEFAULT '[]',
           updated_slugs TEXT NOT NULL DEFAULT '[]',
           protected_slugs TEXT NOT NULL DEFAULT '[]',
           error TEXT
         );
CREATE INDEX idx_knowledge_digest_runs_started
           ON knowledge_digest_runs(started_at);
