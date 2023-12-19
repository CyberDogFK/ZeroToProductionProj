ALTER TABLE issue_delivery_queue ADD COLUMN left_sending_tries INT NOT NULL DEFAULT 0;
