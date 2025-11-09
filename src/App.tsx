import { debug } from "@tauri-apps/plugin-log";
import { Wifi } from "lucide-react";
import { useEffect, useState } from "react";
import { ReceiveFile } from "@/components/ReceiveFile";
import { SendFile } from "@/components/SendFile";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getDeviceName, initNode } from "@/lib/api";

function App() {
	const [nodeId, setNodeId] = useState<string | null>(null);
	const [deviceName, setDeviceName] = useState<string>("");
	const [isInitializing, setIsInitializing] = useState(true);

	useEffect(() => {
		const initialize = async () => {
			try {
				debug("Initializing node");
				const id = await initNode();
				setNodeId(id);

				const name = await getDeviceName();
				setDeviceName(name);
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
			<div className="flex items-center justify-center min-h-screen">
				<div className="text-center space-y-2">
					<div className="animate-spin h-8 w-8 border-4 border-primary border-t-transparent rounded-full mx-auto" />
					<p className="text-sm text-muted-foreground">Initializing...</p>
				</div>
			</div>
		);
	}

	return (
		<div className="min-h-screen bg-background p-4">
			<div className="max-w-2xl mx-auto space-y-6">
				<div className="text-center space-y-2">
					<div className="flex items-center justify-center gap-3">
						<h1 className="text-3xl font-bold">Vegam</h1>
					</div>
					<p className="text-sm text-muted-foreground">P2P File Transfer</p>
				</div>

				<div className="flex items-center justify-center gap-4">
					{deviceName && (
						<div className="flex items-center gap-2 text-sm text-muted-foreground">
							<Wifi className="h-4 w-4" />
							<span>{deviceName}</span>
						</div>
					)}
				</div>

				<Tabs defaultValue="send" className="w-full">
					<TabsList className="grid w-full grid-cols-2">
						<TabsTrigger value="send">Send</TabsTrigger>
						<TabsTrigger value="receive">Receive</TabsTrigger>
					</TabsList>
					<TabsContent value="send" className="mt-6">
						<SendFile />
					</TabsContent>
					<TabsContent value="receive" className="mt-6">
						<ReceiveFile />
					</TabsContent>
				</Tabs>

				{nodeId && (
					<div className="text-center">
						<p className="text-xs text-muted-foreground font-mono">
							Node: {nodeId.slice(0, 16)}...
						</p>
					</div>
				)}
			</div>
		</div>
	);
}

export default App;
