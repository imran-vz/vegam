# Support Cancel Separately From Pause

Production v1 supports Cancellation separately from pause. Pausing preserves Resumable Transfer state; Receiver-side Cancellation deletes the partial download state, while Sender-side Cancellation stops availability for the Transfer Ticket without deleting the Sender's source file.
