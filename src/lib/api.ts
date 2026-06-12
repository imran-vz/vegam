import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Keep in sync with src-tauri/src/engine/types.rs and engine/error.rs.

export type SendTransferStatus =
	| "importing"
	| "available"
	| "paused"
	| "expired"
	| "contentSuspect"
	| "contentChanged"
	| "sourceMissing"
	| "cancelled";

export type ReceiveTransferStatus =
	| "connecting"
	| "downloading"
	| "paused"
	| "stalledRetrying"
	| "verifying"
	| "exporting"
	| "complete"
	| "failed"
	| "cancelled"
	| "noLongerResumable";

export type ConnectionKind = "direct" | "relayed" | "unknown";

export type ErrorCode =
	| "ticketInvalid"
	| "ticketExpired"
	| "notFound"
	| "sourceMissing"
	| "contentMismatch"
	| "alreadyExists"
	| "io"
	| "network"
	| "rejected"
	| "internal";

export interface ApiError {
	code: ErrorCode;
	message: string;
}

export function isApiError(e: unknown): e is ApiError {
	return (
		typeof e === "object" &&
		e !== null &&
		"code" in e &&
		"message" in e &&
		typeof (e as { message: unknown }).message === "string"
	);
}

export interface SendTransferInfo {
	id: string;
	file_name: string;
	size: number;
	source_path: string;
	/** null while importing */
	ticket: string | null;
	issued_at_ms: number | null;
	expires_at_ms: number | null;
	status: SendTransferStatus;
	/** Count only — never identities (ADR 0016). */
	active_receiver_count: number;
	/** 0..1 during importing */
	import_progress: number | null;
	error: string | null;
}

export interface ReceiveTransferInfo {
	id: string;
	file_name: string;
	size: number;
	destination_path: string;
	status: ReceiveTransferStatus;
	local_bytes: number;
	connection_kind: ConnectionKind | null;
	error_code: ErrorCode | null;
	error: string | null;
}

export interface ReceiveProgress {
	id: string;
	local_bytes: number;
	total_bytes: number;
	speed_bps: number;
	connection_kind: ConnectionKind | null;
}

export interface TicketPreview {
	file_name: string;
	size: number;
	issued_at_ms: number;
	expires_at_ms: number;
	/** Advisory — the Sender is authoritative. */
	is_probably_expired: boolean;
}

export type PartialDownloadKind = "tracked" | "orphan";

export interface PartialDownloadEntry {
	entry_id: string;
	kind: PartialDownloadKind;
	file_name: string | null;
	local_bytes: number;
	total_bytes: number | null;
	disk_bytes: number;
	resumable: boolean;
	stale: boolean;
	status: ReceiveTransferStatus | null;
	last_activity_ms: number | null;
}

export interface ResumeAreaReport {
	total_disk_bytes: number;
	entries: PartialDownloadEntry[];
}

export interface Settings {
	display_name: string;
	/** PostHog product telemetry; OFF by default. */
	analytics_enabled: boolean;
}

export interface AppSnapshot {
	settings: Settings;
	send_transfers: SendTransferInfo[];
	receive_transfers: ReceiveTransferInfo[];
}

// Commands

export async function initApp(): Promise<AppSnapshot> {
	return await invoke<AppSnapshot>("init_app");
}

export async function getAppSnapshot(): Promise<AppSnapshot> {
	return await invoke<AppSnapshot>("get_app_snapshot");
}

export async function createSendTransfer(
	filePath: string,
): Promise<SendTransferInfo> {
	return await invoke<SendTransferInfo>("create_send_transfer", { filePath });
}

export async function pauseSendTransfer(
	transferId: string,
): Promise<SendTransferInfo> {
	return await invoke<SendTransferInfo>("pause_send_transfer", { transferId });
}

export async function resumeSendTransfer(
	transferId: string,
): Promise<SendTransferInfo> {
	return await invoke<SendTransferInfo>("resume_send_transfer", {
		transferId,
	});
}

export async function cancelSendTransfer(transferId: string): Promise<void> {
	await invoke<void>("cancel_send_transfer", { transferId });
}

export async function reselectSendSource(
	transferId: string,
	filePath: string,
): Promise<SendTransferInfo> {
	return await invoke<SendTransferInfo>("reselect_send_source", {
		transferId,
		filePath,
	});
}

export async function inspectTicket(ticket: string): Promise<TicketPreview> {
	return await invoke<TicketPreview>("inspect_ticket", { ticket });
}

export async function createReceiveTransfer(
	ticket: string,
	destinationPath: string,
): Promise<ReceiveTransferInfo> {
	return await invoke<ReceiveTransferInfo>("create_receive_transfer", {
		ticket,
		destinationPath,
	});
}

export async function pauseReceiveTransfer(
	transferId: string,
): Promise<ReceiveTransferInfo> {
	return await invoke<ReceiveTransferInfo>("pause_receive_transfer", {
		transferId,
	});
}

export async function resumeReceiveTransfer(
	transferId: string,
): Promise<ReceiveTransferInfo> {
	return await invoke<ReceiveTransferInfo>("resume_receive_transfer", {
		transferId,
	});
}

export async function cancelReceiveTransfer(
	transferId: string,
): Promise<void> {
	await invoke<void>("cancel_receive_transfer", { transferId });
}

export async function listPartialDownloads(): Promise<ResumeAreaReport> {
	return await invoke<ResumeAreaReport>("list_partial_downloads");
}

export async function cleanupPartialDownload(
	entryId: string,
): Promise<ResumeAreaReport> {
	return await invoke<ResumeAreaReport>("cleanup_partial_download", {
		entryId,
	});
}

export async function getSettings(): Promise<Settings> {
	return await invoke<Settings>("get_settings");
}

export async function setDisplayName(displayName: string): Promise<Settings> {
	return await invoke<Settings>("set_display_name", { displayName });
}

export async function setAnalyticsEnabled(
	enabled: boolean,
): Promise<Settings> {
	return await invoke<Settings>("set_analytics_enabled", { enabled });
}

/** User-initiated: copies the local log file to a path the user chose. */
export async function exportDiagnostics(
	destinationPath: string,
): Promise<void> {
	await invoke<void>("export_diagnostics", { destinationPath });
}

// Events

export async function listenToSendTransferUpdates(
	callback: (transfer: SendTransferInfo) => void,
): Promise<UnlistenFn> {
	return await listen<SendTransferInfo>("send-transfer-updated", (event) => {
		callback(event.payload);
	});
}

export async function listenToReceiveTransferUpdates(
	callback: (transfer: ReceiveTransferInfo) => void,
): Promise<UnlistenFn> {
	return await listen<ReceiveTransferInfo>(
		"receive-transfer-updated",
		(event) => {
			callback(event.payload);
		},
	);
}

export async function listenToReceiveTransferProgress(
	callback: (progress: ReceiveProgress) => void,
): Promise<UnlistenFn> {
	return await listen<ReceiveProgress>(
		"receive-transfer-progress",
		(event) => {
			callback(event.payload);
		},
	);
}
