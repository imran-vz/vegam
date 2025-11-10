import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { debug } from "@tauri-apps/plugin-log";
import { Download, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

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
import { formatFileSize, parseError } from "@/lib/utils";

const STEPS = {
	receive: "Receive",
	receiving: "Receiving",
	completed: "Completed",
	failed: "Failed",
} as const;

export function ReceiveFile() {
	const [ticket, setTicket] = useState("");
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [activeTransfer, setActiveTransfer] = useState<TransferInfo | null>(
		null,
	);
	const [step, setStep] = useState<keyof typeof STEPS>("receive");

	// Listen for transfer progress updates
	useEffect(() => {
		const unlisten = listen<TransferInfo>("transfer-progress", (event) => {
			const progress = event.payload;
			if (activeTransfer && progress.id === activeTransfer.id) {
				setActiveTransfer((prev) =>
					prev
						? {
								...prev,
								bytes_transferred: progress.bytes_transferred,
								file_size: progress.file_size || prev.file_size,
								status: progress.status,
							}
						: null,
				);
			}
		});

		return () => {
			unlisten.then((fn) => fn());
		};
	}, [activeTransfer]);

	const handlePaste = async () => {
		try {
			const text = await navigator.clipboard.readText();
			if (text.trim()) {
				setTicket(text.trim());
			}
		} catch (err) {
			setError(parseError(err));
		}
	};

	const handleReceive = async () => {
		if (!ticket.trim()) {
			setError("Please enter a transfer ticket");
			setStep("receive");
			return;
		}

		try {
			setIsLoading(true);
			setError(null);
			setStep("receiving");

			// Parse ticket to get filename
			let defaultFilename = "received_file";
			try {
				const metadata = await parseTicketMetadata(ticket);
				defaultFilename = metadata.filename;
			} catch (e) {
				debug(`Could not parse ticket metadata: ${e}`);
			}

			// Open save dialog with Downloads as default location and proper filename
			const selectedPath = await save({
				defaultPath: `Downloads/${defaultFilename}`,
			});

			if (!selectedPath) {
				setStep("receive");
				setIsLoading(false);
				return;
			}

			debug(`selectedPath: ${selectedPath}`);

			// Start receiving immediately
			const transfer = await receiveFile(ticket, selectedPath);

			setActiveTransfer(transfer);
			setStep("completed");
			setTicket("");

			setTimeout(() => {
				setStep("receive");
			}, 1500);
		} catch (err) {
			setError(parseError(err));
			setStep("failed");
		} finally {
			setIsLoading(false);
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
				{!activeTransfer ? (
					<>
						<div className="space-y-2">
							<textarea
								value={ticket}
								onChange={(e) => setTicket(e.target.value)}
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
							disabled={isLoading || !ticket.trim()}
							className="w-full"
						>
							{isLoading ? (
								<Loader2 className="mr-2 h-4 w-4 animate-spin" />
							) : (
								<Download className="mr-2 h-4 w-4" />
							)}

							{STEPS[step]}
						</Button>
					</>
				) : (
					<div className="space-y-4">
						<div className="p-3 bg-muted rounded-lg">
							<p className="font-medium">{activeTransfer.file_name}</p>
							<p className="text-sm text-muted-foreground">
								{formatFileSize(activeTransfer.file_size)}
							</p>
						</div>

						{activeTransfer.status === "failed" && activeTransfer.error ? (
							<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
								<p className="font-medium">Transfer Failed</p>
								<p className="text-xs mt-1">{activeTransfer.error}</p>
							</div>
						) : activeTransfer.status === "completed" ? (
							<div className="p-3 text-sm text-green-700 bg-green-100 rounded-lg">
								<p className="font-medium">Transfer Completed</p>
							</div>
						) : (
							<div className="space-y-2">
								<div className="flex justify-between text-sm">
									<span>
										{activeTransfer.status === "inprogress"
											? "Downloading..."
											: "Preparing..."}
									</span>
									<span>
										{activeTransfer.file_size > 0
											? Math.round(
													(activeTransfer.bytes_transferred /
														activeTransfer.file_size) *
														100,
												)
											: 0}
										%
									</span>
								</div>
								<Progress
									value={
										activeTransfer.file_size > 0
											? (activeTransfer.bytes_transferred /
													activeTransfer.file_size) *
												100
											: 0
									}
								/>
							</div>
						)}

						{(activeTransfer.status === "completed" ||
							activeTransfer.status === "failed") && (
							<Button
								variant="outline"
								onClick={() => setActiveTransfer(null)}
								className="w-full"
							>
								Receive Another File
							</Button>
						)}
					</div>
				)}

				{error && (
					<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
						{error}
					</div>
				)}
			</CardContent>
		</Card>
	);
}
