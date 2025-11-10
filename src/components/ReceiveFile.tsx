import { save } from "@tauri-apps/plugin-dialog";
import { debug } from "@tauri-apps/plugin-log";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { Download, Loader2 } from "lucide-react";
import { useEffect, useReducer, useState } from "react";

import { QRScanner } from "@/components/QRScanner";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import {
	listenToTransferProgress,
	listenToTransferUpdates,
	parseTicketMetadata,
	receiveFile,
} from "@/lib/api";
import { receiveFileReducer } from "@/lib/state-machines";
import { formatFileSize, formatTransferSpeed, parseError } from "@/lib/utils";

export function ReceiveFile() {
	const [state, dispatch] = useReducer(receiveFileReducer, {
		type: "idle",
		ticket: "",
	});
	const [showScanner, setShowScanner] = useState(false);

	// Listen for transfer progress and completion updates
	useEffect(() => {
		const unlistenProgress = listenToTransferProgress((progress) => {
			if (state.type === "downloading" && progress.id === state.transfer.id) {
				console.log({ progress });
				// Backend already throttles to 100ms, no need for frontend throttling
				dispatch({
					type: "PROGRESS_UPDATE",
					bytesTransferred: progress.bytes_transferred,
					fileSize: progress.file_size,
					speed_bps: progress.speed_bps,
				});
			}
		});

		const unlistenUpdate = listenToTransferUpdates((transfer) => {
			if (state.type === "downloading" && transfer.id === state.transfer.id) {
				if (transfer.status === "completed") {
					dispatch({ type: "DOWNLOAD_COMPLETED", transfer });
				} else if (transfer.status === "failed") {
					dispatch({
						type: "ERROR",
						error: transfer.error || "Transfer failed",
					});
				}
			}
		});

		return () => {
			unlistenProgress.then((fn) => fn());
			unlistenUpdate.then((fn) => fn());
		};
	}, [state]);

	// Auto-reset after successful download
	useEffect(() => {
		if (state.type === "success") {
			const timer = setTimeout(() => {
				dispatch({ type: "RESET" });
			}, 1500);
			return () => clearTimeout(timer);
		}
	}, [state.type]);

	const handlePaste = async () => {
		try {
			const text = await readText();
			if (text?.trim()) {
				dispatch({ type: "SET_TICKET", ticket: text.trim() });
			}
		} catch (err) {
			dispatch({ type: "ERROR", error: parseError(err) });
		}
	};

	const handleQRScan = (ticket: string) => {
		dispatch({ type: "SET_TICKET", ticket: ticket.trim() });
		setShowScanner(false);
	};

	const handleReceive = async () => {
		dispatch({ type: "RECEIVE" });

		if (state.type !== "idle" || !state.ticket.trim()) {
			return;
		}

		const ticket = state.ticket;

		try {
			// Parse ticket to get filename
			let defaultFilename = "received_file";
			try {
				const metadata = await parseTicketMetadata(ticket);
				defaultFilename = metadata.filename;
				dispatch({ type: "METADATA_PARSED", filename: defaultFilename });
			} catch (e) {
				debug(`Could not parse ticket metadata: ${e}`);
				dispatch({ type: "METADATA_PARSE_FAILED" });
			}

			// Open save dialog with Downloads as default location and proper filename
			const selectedPath = await save({
				defaultPath: `Downloads/${defaultFilename}`,
			});

			if (!selectedPath) {
				dispatch({ type: "PATH_SELECTION_CANCELLED" });
				return;
			}

			debug(`selectedPath: ${selectedPath}`);
			dispatch({ type: "PATH_SELECTED", path: selectedPath });

			// Start receiving immediately - this returns instantly with pending status
			const transfer = await receiveFile(ticket, selectedPath);
			dispatch({ type: "DOWNLOAD_STARTED", transfer });

			// Completion will be handled by transfer-update event listener
		} catch (err) {
			dispatch({ type: "ERROR", error: parseError(err) });
		}
	};

	const isLoading =
		state.type === "parsing_metadata" ||
		state.type === "awaiting_path" ||
		state.type === "downloading";

	const getButtonText = () => {
		switch (state.type) {
			case "parsing_metadata":
			case "awaiting_path":
			case "downloading":
				return "Receiving";
			case "success":
				return "Completed";
			default:
				return "Receive";
		}
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Receive File</CardTitle>
				<CardDescription>
					Paste a transfer ticket to receive a file
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				{state.type !== "downloading" && state.type !== "success" ? (
					<>
						<div className="space-y-2">
							<textarea
								value={state.type === "idle" ? state.ticket : ""}
								onChange={(e) =>
									dispatch({ type: "SET_TICKET", ticket: e.target.value })
								}
								placeholder="Paste transfer ticket here..."
								className="w-full h-24 p-3 text-sm font-mono border rounded-lg resize-none"
							/>
							<div className="grid grid-cols-2 gap-2">
								<Button
									variant="outline"
									size="sm"
									onClick={handlePaste}
									className="w-full"
								>
									Paste from Clipboard
								</Button>
								<Button
									variant="outline"
									size="sm"
									onClick={() => setShowScanner(!showScanner)}
									className="w-full"
								>
									{showScanner ? "Hide Scanner" : "Scan QR Code"}
								</Button>
							</div>
						</div>

						{showScanner && (
							<QRScanner
								onScan={handleQRScan}
								onError={(err) => dispatch({ type: "ERROR", error: err })}
							/>
						)}

						<Button
							onClick={handleReceive}
							disabled={
								isLoading || (state.type === "idle" && !state.ticket.trim())
							}
							className="w-full"
						>
							{isLoading ? (
								<Loader2 className="size-4 animate-spin" />
							) : (
								<Download className="size-4" />
							)}

							{getButtonText()}
						</Button>
					</>
				) : (
					<div className="space-y-4">
						<div className="p-3 bg-muted rounded-lg">
							<p className="font-medium">{state.transfer.file_name}</p>
							<p className="text-sm text-muted-foreground">
								{formatFileSize(state.transfer.file_size)}
							</p>
						</div>

						{state.type === "success" ? (
							<div className="p-3 text-sm text-green-700 bg-green-100 rounded-lg">
								<p className="font-medium">Transfer Completed</p>
							</div>
						) : (
							<div className="space-y-2">
								<div className="flex justify-between text-sm">
									<span>
										{state.transfer.status === "inprogress"
											? "Downloading..."
											: "Downloaded"}
									</span>
									<span>
										{state.transfer.file_size > 0
											? Math.round(
													(state.transfer.bytes_transferred /
														state.transfer.file_size) *
														100,
												)
											: 0}
										%
									</span>
								</div>
								<Progress
									value={
										state.transfer.file_size > 0
											? (state.transfer.bytes_transferred /
													state.transfer.file_size) *
												100
											: 0
									}
								/>
								{["pending", "inprogress"].includes(state.transfer.status) && (
									<div className="text-xs text-muted-foreground text-right">
										{formatTransferSpeed(state.transfer.speed_bps)}
									</div>
								)}
							</div>
						)}

						{state.type === "success" && (
							<Button
								variant="outline"
								onClick={() => dispatch({ type: "RESET" })}
								className="w-full"
							>
								Receive Another File
							</Button>
						)}
					</div>
				)}

				{state.type === "error" && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{state.error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
