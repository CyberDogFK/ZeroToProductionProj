-- Add migration script here
ALTER TABLE issue_delivery_queue DROP COLUMN execute_after_duration;
