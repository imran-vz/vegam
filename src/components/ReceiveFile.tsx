import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { debug } from "@tauri-apps/plugin-log";
import { Download, Loader2 } from "lucide-react";
import { useEffect, useReducer } from "react";

import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { parseTicketMetadata, receiveFile, type TransferInfo } from "@/lib/api";
import { receiveFileReducer } from "@/lib/state-machines";
import { formatFileSize, parseError } from "@/lib/utils";

export function ReceiveFile() {
	const [state, dispatch] = useReducer(receiveFileReducer, {
		type: "idle",
		ticket: "",
	});

	// Listen for transfer progress updates
	useEffect(() => {
		const unlisten = listen<TransferInfo>("transfer-progress", (event) => {
			const progress = event.payload;
			if (state.type === "downloading" && progress.id === state.transfer.id) {
				dispatch({
					type: "PROGRESS_UPDATE",
					bytesTransferred: progress.bytes_transferred,
					fileSize: progress.file_size,
				});
			}
		});

		return () => {
			unlisten.then((fn) => fn());
		};
	}, [state]);

	const handlePaste = async () => {
		try {
			const text = await navigator.clipboard.readText();
			if (text.trim()) {
				dispatch({ type: "SET_TICKET", ticket: text.trim() });
			}
		} catch (err) {
			dispatch({ type: "ERROR", error: parseError(err) });
		}
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

			// Start receiving immediately
			const transfer = await receiveFile(ticket, selectedPath);
			dispatch({ type: "DOWNLOAD_STARTED", transfer });

			// Auto-reset after completion
			setTimeout(() => {
				dispatch({ type: "RESET" });
			}, 1500);
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
							<Button
								variant="outline"
								size="sm"
								onClick={handlePaste}
								className="w-full"
							>
								Paste from Clipboard
							</Button>
						</div>

						<Button
							onClick={handleReceive}
							disabled={
								isLoading || (state.type === "idle" && !state.ticket.trim())
							}
							className="w-full"
						>
							{isLoading ? (
								<Loader2 className="mr-2 h-4 w-4 animate-spin" />
							) : (
								<Download className="mr-2 h-4 w-4" />
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
											: "Preparing..."}
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
