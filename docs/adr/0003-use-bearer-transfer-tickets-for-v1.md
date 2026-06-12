# Use Bearer Transfer Tickets for V1

Production v1 Transfer Tickets are bearer credentials: anyone with a valid ticket can receive the file while the Sender makes it available. A single Transfer Ticket may be used by multiple Receivers until it expires or the Sender cancels availability. Recipient-bound or single-use tickets would provide stronger control, but they require identity exchange or extra state before sending and complicate the first desktop production path; v1 will instead make the sharing model clear in the UI.
