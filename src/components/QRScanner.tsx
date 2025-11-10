import { cancel, Format, scan } from "@tauri-apps/plugin-barcode-scanner";
import { QrCode, X } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

interface QRScannerProps {
	onScan: (result: string) => void;
	onError?: (error: string) => void;
}

export function QRScanner({ onScan, onError }: QRScannerProps) {
	const [isScanning, setIsScanning] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const startScanning = async () => {
		try {
			setError(null);
			setIsScanning(true);

			// windowed: true makes webview transparent to show camera underneath
			const result = await scan({
				windowed: true,
				formats: [Format.QRCode],
			});

			if (result?.content) {
				onScan(result.content);
			}

			setIsScanning(false);
		} catch (err) {
			const errorMsg =
				err instanceof Error ? err.message : "Failed to start camera";
			setError(errorMsg);
			onError?.(errorMsg);
			setIsScanning(false);
		}
	};

	const stopScanning = async () => {
		try {
			await cancel();
		} catch (err) {
			console.error("Error stopping scanner:", err);
		}
		setIsScanning(false);
	};

	return (
		<div className="space-y-4">
			{!isScanning ? (
				<Button onClick={startScanning} className="w-full" variant="outline">
					<QrCode className="size-4" />
					Scan QR Code
				</Button>
			) : (
				<div className="space-y-2">
					<div className="w-full h-64 bg-black/20 rounded-lg flex items-center justify-center">
						<p className="text-sm text-muted-foreground">
							Point camera at QR code
						</p>
					</div>
					<Button onClick={stopScanning} className="w-full" variant="outline">
						<X className="size-4" />
						Cancel Scan
					</Button>
				</div>
			)}

			{error && (
				<div className="p-3 text-sm text-destructive bg-destructive/10 rounded-lg">
					{error}
				</div>
			)}
		</div>
	);
}
