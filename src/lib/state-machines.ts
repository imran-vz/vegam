import type { BlobTicketInfo, TransferInfo } from "./api";

// SendFile State Machine
export type SendFileState =
	| { type: "idle" }
	| { type: "selecting" }
	| { type: "generating" }
	| { type: "success"; data: BlobTicketInfo }
	| { type: "error"; error: string };

export type SendFileEvent =
	| { type: "SELECT_FILE" }
	| { type: "FILE_SELECTED"; path: string }
	| { type: "FILE_SELECTION_CANCELLED" }
	| { type: "TICKET_GENERATED"; ticket: BlobTicketInfo }
	| { type: "ERROR"; error: string }
	| { type: "RESET" };

export function sendFileReducer(
	state: SendFileState,
	event: SendFileEvent,
): SendFileState {
	switch (state.type) {
		case "idle":
			if (event.type === "SELECT_FILE") {
				return { type: "selecting" };
			}
			break;

		case "selecting":
			if (event.type === "FILE_SELECTED") {
				return { type: "generating" };
			}
			if (event.type === "FILE_SELECTION_CANCELLED") {
				return { type: "idle" };
			}
			if (event.type === "ERROR") {
				return { type: "error", error: event.error };
			}
			break;

		case "generating":
			if (event.type === "TICKET_GENERATED") {
				return { type: "success", data: event.ticket };
			}
			if (event.type === "ERROR") {
				return { type: "error", error: event.error };
			}
			break;

		case "success":
			if (event.type === "RESET") {
				return { type: "idle" };
			}
			break;

		case "error":
			if (event.type === "RESET") {
				return { type: "idle" };
			}
			break;
	}

	return state;
}

// ReceiveFile State Machine
export type ReceiveFileState =
	| { type: "idle"; ticket: string }
	| { type: "parsing_metadata" }
	| { type: "awaiting_path"; filename: string }
	| { type: "downloading"; transfer: TransferInfo }
	| { type: "success"; transfer: TransferInfo }
	| { type: "error"; error: string };

export type ReceiveFileEvent =
	| { type: "SET_TICKET"; ticket: string }
	| { type: "RECEIVE" }
	| { type: "METADATA_PARSED"; filename: string }
	| { type: "METADATA_PARSE_FAILED" }
	| { type: "PATH_SELECTED"; path: string }
	| { type: "PATH_SELECTION_CANCELLED" }
	| { type: "DOWNLOAD_STARTED"; transfer: TransferInfo }
	| { type: "PROGRESS_UPDATE"; bytesTransferred: number; fileSize: number }
	| { type: "DOWNLOAD_COMPLETED"; transfer: TransferInfo }
	| { type: "ERROR"; error: string }
	| { type: "RESET" };

export function receiveFileReducer(
	state: ReceiveFileState,
	event: ReceiveFileEvent,
): ReceiveFileState {
	switch (state.type) {
		case "idle":
			if (event.type === "SET_TICKET") {
				return { type: "idle", ticket: event.ticket };
			}
			if (event.type === "RECEIVE") {
				if (!state.ticket.trim()) {
					return {
						type: "error",
						error: "Please enter a transfer ticket",
					};
				}
				return { type: "parsing_metadata" };
			}
			break;

		case "parsing_metadata":
			if (event.type === "METADATA_PARSED") {
				return { type: "awaiting_path", filename: event.filename };
			}
			if (event.type === "METADATA_PARSE_FAILED") {
				return { type: "awaiting_path", filename: "received_file" };
			}
			if (event.type === "ERROR") {
				return { type: "error", error: event.error };
			}
			break;

		case "awaiting_path":
			if (event.type === "PATH_SELECTED") {
				// State will transition to downloading when backend responds
				return state;
			}
			if (event.type === "PATH_SELECTION_CANCELLED") {
				return { type: "idle", ticket: "" };
			}
			if (event.type === "DOWNLOAD_STARTED") {
				return { type: "downloading", transfer: event.transfer };
			}
			if (event.type === "ERROR") {
				return { type: "error", error: event.error };
			}
			break;

		case "downloading":
			if (event.type === "PROGRESS_UPDATE") {
				return {
					type: "downloading",
					transfer: {
						...state.transfer,
						bytes_transferred: event.bytesTransferred,
						file_size: event.fileSize || state.transfer.file_size,
					},
				};
			}
			if (event.type === "DOWNLOAD_COMPLETED") {
				return { type: "success", transfer: event.transfer };
			}
			if (event.type === "ERROR") {
				return { type: "error", error: event.error };
			}
			break;

		case "success":
			if (event.type === "RESET") {
				return { type: "idle", ticket: "" };
			}
			break;

		case "error":
			if (event.type === "RESET") {
				return { type: "idle", ticket: "" };
			}
			break;
	}

	return state;
}
