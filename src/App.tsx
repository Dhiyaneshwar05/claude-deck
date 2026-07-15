import { Sidebar } from "./components/sidebar/Sidebar";
import { ChatView } from "./components/chat/ChatView";
import { PermissionCenter } from "./components/permissions/PermissionCenter";
import { useHealthPoll } from "./hooks/useHealthPoll";
import { useSessionEvents } from "./hooks/useSessionEvents";
import { usePermissionEvents } from "./hooks/usePermissionEvents";

export default function App() {
  useHealthPoll();
  useSessionEvents();
  usePermissionEvents();

  return (
    <div className="h-screen w-screen grid grid-cols-[280px_1fr] bg-zinc-950">
      <Sidebar />
      <main className="h-full overflow-hidden bg-zinc-900/30">
        <ChatView />
      </main>
      <PermissionCenter />
    </div>
  );
}
