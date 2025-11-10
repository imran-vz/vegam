import { debug } from "@tauri-apps/plugin-log";
import { Wifi, WifiOff } from "lucide-react";
import { useEffect, useState } from "react";
import { ReceiveFile } from "@/components/ReceiveFile";
import { SendFile } from "@/components/SendFile";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getDeviceName, getRelayStatus, initNode, type RelayStatus } from "@/lib/api";

function App() {
	const [nodeId, setNodeId] = useState<string | null>(null);
	const [deviceName, setDeviceName] = useState<string>("");
	const [relayStatus, setRelayStatus] = useState<RelayStatus | null>(null);
	const [isInitializing, setIsInitializing] = useState(true);

	useEffect(() => {
		const initialize = async () => {
			try {
				debug("Initializing node");
				const id = await initNode();
				setNodeId(id);

				const name = await getDeviceName();
				setDeviceName(name);

				const status = await getRelayStatus();
				setRelayStatus(status);
			} catch (error) {
				console.error("Failed to initialize:", error);
			} finally {
				setIsInitializing(false);
			}
		};

		initialize();
	}, []);

	if (isInitializing) {
		return (
			<div className="flex items-center justify-center min-h-screen bg-background">
				<div className="text-center space-y-2">
					<div className="animate-spin h-8 w-8 border-4 border-primary border-t-transparent rounded-full mx-auto" />
					<p className="text-sm text-muted-foreground">Initializing...</p>
				</div>
			</div>
		);
	}

	return (
		<div className="min-h-screen bg-background p-4 md:p-6">
			<div className="max-w-2xl mx-auto space-y-4 md:space-y-6">
				<div className="text-center space-y-1">
					<div className="flex items-center justify-center gap-3">
						<h1 className="text-2xl md:text-3xl font-bold">Vegam</h1>
					</div>
					<p className="text-xs md:text-sm text-muted-foreground">
						P2P File Transfer
					</p>
				</div>

				{deviceName && (
					<div className="flex items-center justify-center gap-4">
						<div className="flex items-center gap-2 text-xs md:text-sm text-muted-foreground">
							<Wifi className="h-3 w-3 md:h-4 md:w-4" />
							<span>{deviceName}</span>
						</div>
						{relayStatus && (
							<div className="flex items-center gap-2 text-xs text-muted-foreground">
								{relayStatus.connected ? (
									<>
										<Wifi className="h-3 w-3 text-green-600" />
										<span>Relay</span>
									</>
								) : (
									<>
										<WifiOff className="h-3 w-3 text-amber-600" />
										<span>Direct only</span>
									</>
								)}
							</div>
						)}
					</div>
				)}

				<Tabs defaultValue="send" className="w-full">
					<TabsList className="grid w-full grid-cols-2">
						<TabsTrigger value="send">Send</TabsTrigger>
						<TabsTrigger value="receive">Receive</TabsTrigger>
					</TabsList>
					<TabsContent value="send" className="mt-4 md:mt-6">
						<SendFile />
					</TabsContent>
					<TabsContent value="receive" className="mt-4 md:mt-6">
						<ReceiveFile />
					</TabsContent>
				</Tabs>

				{nodeId && (
					<div className="text-center pb-2">
						<p className="text-[10px] md:text-xs text-muted-foreground font-mono">
							Node: {nodeId.slice(0, 16)}...
						</p>
					</div>
				)}
			</div>
		</div>
	);
}

export default App;
