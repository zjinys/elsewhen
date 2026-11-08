-- Schema 74 existed before prompt_generation was added to its source file.
-- Startup reconciles this single historical column before advancing to 75.
-- No worker may migrate: its version check waits for foreground initialization.
SELECT 1;
