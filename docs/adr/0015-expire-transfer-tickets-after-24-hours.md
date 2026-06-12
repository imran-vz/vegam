# Expire Transfer Tickets After 24 Hours

Production v1 Transfer Tickets expire automatically after 24 hours by default, and the Sender can cancel availability earlier. Since Transfer Tickets are bearer credentials, expiration limits accidental long-lived access without requiring accounts, identity exchange, or recipient-bound tickets in v1. Expiration blocks new Receivers, but a Receiver that started the Transfer before expiration may resume after expiration if it still has valid partial state.
