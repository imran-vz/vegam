import { Html5Qrcode } from "html5-qrcode";
import { QrCode, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";

interface QRScannerProps {
	onScan: (result: string) => void;
	onError?: (error: string) => void;
}

export function QRScanner({ onScan, onError }: QRScannerProps) {
	const [isScanning, setIsScanning] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const scannerRef = useRef<Html5Qrcode | null>(null);
	const qrCodeRegionId = "qr-reader";

	const startScanning = async () => {
		try {
			setError(null);
			setIsScanning(true);

			const scanner = new Html5Qrcode(qrCodeRegionId);
			scannerRef.current = scanner;

			await scanner.start(
				{ facingMode: "environment" },
				{
					fps: 10,
					qrbox: { width: 250, height: 250 },
				},
				(decodedText) => {
					// Success callback
					onScan(decodedText);
					stopScanning();
				},
				() => {
					// Error callback (typically just "No QR code found")
					// Don't show these as errors, they're expected
				},
			);
		} catch (err) {
			const errorMsg =
				err instanceof Error ? err.message : "Failed to start camera";
			setError(errorMsg);
			onError?.(errorMsg);
			setIsScanning(false);
		}
	};

	const stopScanning = async () => {
		if (scannerRef.current) {
			try {
				await scannerRef.current.stop();
				scannerRef.current = null;
			} catch (err) {
				console.error("Error stopping scanner:", err);
			}
		}
		setIsScanning(false);
	};

	useEffect(() => {
		return () => {
			if (scannerRef.current) {
				scannerRef.current.stop().catch(console.error);
			}
		};
	}, []);

	return (
		<div className="space-y-4">
			{!isScanning ? (
				<Button onClick={startScanning} className="w-full" variant="outline">
					<QrCode className="size-4" />
					Scan QR Code
				</Button>
			) : (
				<div className="space-y-2">
					<div
						id={qrCodeRegionId}
						className="w-full rounded-lg overflow-hidden"
					/>
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
