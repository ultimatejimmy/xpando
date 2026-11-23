import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, Panda } from "lucide-react";

export default function App() {
  const [files, setFiles] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string[]>("list_match_files")
      .then(setFiles)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="app">
      <header className="header">
        <Panda size={24} strokeWidth={2} aria-hidden="true" />
        <span className="wordmark">xpando</span>
      </header>
      <main className="content">
        <h1>Match files</h1>
        {error && <p className="error">{error}</p>}
        {!error && files.length === 0 && <p>No espanso match files found.</p>}
        <ul className="files">
          {files.map((f) => (
            <li key={f}>
              <FileText size={16} aria-hidden="true" />
              <code>{f}</code>
            </li>
          ))}
        </ul>
      </main>
    </div>
  );
}
