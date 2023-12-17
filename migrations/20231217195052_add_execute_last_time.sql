-- Add migration script here
ALTER TABLE issue_delivery_queue ADD COLUMN execute_last_time timestamptz;
