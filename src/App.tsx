import { Sidebar } from "./components/sidebar/Sidebar";
import { ChatView } from "./components/chat/ChatView";
import { useHealthPoll } from "./hooks/useHealthPoll";
import { useSessionEvents } from "./hooks/useSessionEvents";

export default function App() {
  useHealthPoll();
  useSessionEvents();

  return (
    <div className="h-screen w-screen grid grid-cols-[280px_1fr] bg-zinc-950">
      <Sidebar />
      <main className="h-full overflow-hidden bg-zinc-900/30">
        <ChatView />
      </main>
    </div>
  );
}
