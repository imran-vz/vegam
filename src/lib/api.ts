import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface TransferInfo {
	id: string;
	file_name: string;
	file_size: number;
	bytes_transferred: number;
	status: "pending" | "inprogress" | "completed" | "failed" | "cancelled";
	error: string | null;
	direction: "send" | "receive";
}

export interface PeerInfo {
	node_id: string;
	device_name: string;
	last_seen: number;
}

export interface BlobTicketInfo {
	ticket: string;
	file_name: string;
	file_size: number;
	transfer_id: string;
}

export async function initNode(): Promise<string> {
	return await invoke<string>("init_node");
}

export async function getNodeId(): Promise<string> {
	return await invoke<string>("get_node_id");
}

export async function sendFile(filePath: string): Promise<BlobTicketInfo> {
	return await invoke<BlobTicketInfo>("send_file", { filePath });
}

export async function receiveFile(
	ticket: string,
	outputPath: string,
): Promise<TransferInfo> {
	return await invoke<TransferInfo>("receive_file", { ticket, outputPath });
}

export async function getTransferStatus(
	transferId: string,
): Promise<TransferInfo | null> {
	return await invoke<TransferInfo | null>("get_transfer_status", {
		transferId,
	});
}

export async function listPeers(): Promise<PeerInfo[]> {
	return await invoke<PeerInfo[]>("list_peers");
}

export async function getDeviceName(): Promise<string> {
	return await invoke<string>("get_device_name");
}

export async function listenToTransferUpdates(
	callback: (transfer: TransferInfo) => void,
): Promise<UnlistenFn> {
	return await listen<TransferInfo>("transfer-update", (event) => {
		callback(event.payload);
	});
}

export async function listenToTransferProgress(
	callback: (transfer: TransferInfo) => void,
): Promise<UnlistenFn> {
	return await listen<TransferInfo>("transfer-progress", (event) => {
		callback(event.payload);
	});
}
