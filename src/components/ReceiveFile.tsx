import { save } from "@tauri-apps/plugin-dialog";
import { debug, error as logError } from "@tauri-apps/plugin-log";
import { Download } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { receiveFile, type TransferInfo } from "@/lib/api";
import { formatFileSize } from "@/lib/utils";

export function ReceiveFile() {
	const [ticket, setTicket] = useState("");
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [showApprovalDialog, setShowApprovalDialog] = useState(false);
	const [pendingTransfer, setPendingTransfer] = useState<{
		ticket: string;
		outputPath: string;
	} | null>(null);
	const [activeTransfer, setActiveTransfer] = useState<TransferInfo | null>(
		null,
	);

	const handlePaste = async () => {
		try {
			const text = await navigator.clipboard.readText();
			if (text.trim()) {
				setTicket(text.trim());
			}
		} catch (err) {
			setError("Failed to read clipboard");
			console.error(err);
		}
	};

	const handleReceive = async () => {
		if (!ticket.trim()) {
			setError("Please enter a transfer ticket");
			return;
		}

		try {
			setIsLoading(true);
			setError(null);

			// Open save dialog
			const selectedPath = await save({
				defaultPath: "received_file",
			});

			if (!selectedPath) {
				return;
			}

			debug(`selectedPath: ${selectedPath}`);

			const outputPath = selectedPath;

			// Show approval dialog
			setPendingTransfer({ ticket, outputPath });
			setShowApprovalDialog(true);
		} catch (err) {
			if (err instanceof Error) {
				logError(err.message);
				setError(err.message);
			} else if (typeof err === "string") {
				logError(err);
				setError(err);
			} else {
				logError("Failed to start transfer");
				setError("Failed to start transfer");
			}
		} finally {
			setIsLoading(false);
		}
	};

	const handleApprove = async () => {
		if (!pendingTransfer) return;

		try {
			setShowApprovalDialog(false);
			setIsLoading(true);

			const transfer = await receiveFile(
				pendingTransfer.ticket,
				pendingTransfer.outputPath,
			);

			setActiveTransfer(transfer);
			setTicket("");
			setPendingTransfer(null);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to receive file");
		} finally {
			setIsLoading(false);
		}
	};

	const handleReject = () => {
		setShowApprovalDialog(false);
		setPendingTransfer(null);
		setIsLoading(false);
	};

	return (
		<>
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
								<Download className="mr-2 h-4 w-4" />
								{isLoading ? "Processing..." : "Receive File"}
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

							<div className="space-y-2">
								<div className="flex justify-between text-sm">
									<span>Downloading...</span>
									<span>
										{Math.round(
											(activeTransfer.bytes_transferred /
												activeTransfer.file_size) *
												100,
										)}
										%
									</span>
								</div>
								<Progress
									value={
										(activeTransfer.bytes_transferred /
											activeTransfer.file_size) *
										100
									}
								/>
							</div>

							{activeTransfer.status === "completed" && (
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

			<Dialog open={showApprovalDialog} onOpenChange={setShowApprovalDialog}>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>Approve Incoming Transfer</DialogTitle>
						<DialogDescription>
							Do you want to receive this file?
						</DialogDescription>
					</DialogHeader>
					<div className="py-4">
						<p className="text-sm text-muted-foreground">
							A peer wants to send you a file. Review the details and approve to
							start the transfer.
						</p>
					</div>
					<DialogFooter>
						<Button variant="outline" onClick={handleReject}>
							Reject
						</Button>
						<Button onClick={handleApprove}>Approve & Download</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
}
