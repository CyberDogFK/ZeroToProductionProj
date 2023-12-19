-- Add migration script here
ALTER TABLE issue_delivery_queue drop COLUMN execute_after_duration;

ALTER TABLE issue_delivery_queue ADD COLUMN execute_after_duration int NOT NULL;
