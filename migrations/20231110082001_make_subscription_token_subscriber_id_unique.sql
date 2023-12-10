ALTER TABLE subscription_tokens ADD CONSTRAINT subscriber_id_subscription UNIQUE (subscriber_id);
