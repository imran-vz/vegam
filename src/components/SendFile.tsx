import { open } from "@tauri-apps/plugin-dialog";
import { debug } from "@tauri-apps/plugin-log";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Check, Copy, File, Loader2 } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useEffect, useReducer, useState } from "react";

import { Badge } from "@/components/ui/badge";
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
	sendFile,
} from "@/lib/api";
import { sendFileReducer } from "@/lib/state-machines";
import {
	formatFileSize,
	formatTransferSpeed,
	parseError,
} from "@/lib/utils";

export function SendFile() {
	const [state, dispatch] = useReducer(sendFileReducer, { type: "idle" });
	const [copied, setCopied] = useState(false);

	// Listen for transfer progress and completion updates
	useEffect(() => {
		const unlistenProgress = listenToTransferProgress((progress) => {
			if (state.type === "uploading" && progress.id === state.transfer.id) {
				dispatch({
					type: "PROGRESS_UPDATE",
					bytesTransferred: progress.bytes_transferred,
					fileSize: progress.file_size,
					speed_bps: progress.speed_bps,
				});
			}
		});

		const unlistenUpdate = listenToTransferUpdates((transfer) => {
			if (state.type === "uploading" && transfer.id === state.transfer.id) {
				if (transfer.status === "completed") {
					// Will get ticket from sendFile promise
					return;
				}
			} else if (state.type === "selecting" && transfer.direction === "send") {
				// Initial transfer update
				dispatch({ type: "UPLOAD_STARTED", transfer });
			}
		});

		return () => {
			unlistenProgress.then((fn) => fn());
			unlistenUpdate.then((fn) => fn());
		};
	}, [state]);

	const handleSelectFile = async () => {
		try {
			dispatch({ type: "SELECT_FILE" });

			const selected = await open({
				multiple: false,
				directory: false,
			});

			if (!selected) {
				dispatch({ type: "FILE_SELECTION_CANCELLED" });
				return;
			}

			debug(`selected file: ${selected}`);
			dispatch({ type: "FILE_SELECTED", path: selected });

			const ticket = await sendFile(selected);
			dispatch({ type: "TICKET_GENERATED", ticket });
		} catch (err) {
			dispatch({ type: "ERROR", error: parseError(err) });
		}
	};

	const handleCopyTicket = async () => {
		if (state.type !== "success") return;

		await writeText(state.data.ticket);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	};

	const isLoading =
		state.type === "selecting" || state.type === "uploading";

	const getButtonText = () => {
		switch (state.type) {
			case "selecting":
				return "Selecting File";
			case "uploading":
				return "Uploading...";
			default:
				return "Select File";
		}
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Send File</CardTitle>
				<CardDescription>
					Select a file to generate a transfer ticket
				</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				{state.type !== "success" && state.type !== "uploading" ? (
					<Button
						onClick={handleSelectFile}
						disabled={isLoading}
						className="w-full"
					>
						{isLoading ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<File className="size-4" />
						)}

						{getButtonText()}
					</Button>
				) : state.type === "uploading" ? (
					<div className="space-y-4">
						<div className="p-3 bg-muted rounded-lg">
							<p className="font-medium">{state.transfer.file_name}</p>
							<p className="text-sm text-muted-foreground">
								{formatFileSize(state.transfer.file_size)}
							</p>
						</div>

						<div className="space-y-2">
							<div className="flex justify-between text-sm">
								<span>Uploading...</span>
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
							{state.transfer.speed_bps > 0 && (
								<div className="text-xs text-muted-foreground text-right">
									{formatTransferSpeed(state.transfer.speed_bps)}
								</div>
							)}
						</div>
					</div>
				) : (
					<div className="space-y-4">
						<div className="flex items-center justify-between p-3 bg-muted rounded-lg">
							<div>
								<p className="font-medium">{state.data.file_name}</p>
								<p className="text-sm text-muted-foreground">
									{formatFileSize(state.data.file_size)}
								</p>
							</div>
							<Badge>Ready</Badge>
						</div>

						<div className="space-y-2">
							<label htmlFor="transfer-ticket" className="text-sm font-medium">
								Transfer Ticket
							</label>
							<div className="flex gap-2">
								<div
									id="transfer-ticket"
									className="flex-1 p-2 bg-muted rounded text-xs font-mono break-all"
								>
									{state.data.ticket.slice(0, 80)}...
								</div>
								<Button
									size="icon"
									variant="outline"
									onClick={handleCopyTicket}
								>
									{copied ? (
										<Check className="size-4" />
									) : (
										<Copy className="size-4" />
									)}
								</Button>
							</div>
						</div>

						<div className="flex justify-center p-4 bg-white rounded-lg">
							<QRCodeSVG value={state.data.ticket} size={200} level="M" />
						</div>

						<Button
							variant="outline"
							onClick={() => dispatch({ type: "RESET" })}
							className="w-full"
						>
							Send Another File
						</Button>
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
