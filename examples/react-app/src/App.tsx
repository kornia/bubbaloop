// App.tsx
import React, { useState } from 'react';
import './App.css';
import StreamViewerWebsocket from './components/StreamViewerWebsocket';
import InferenceResultDisplay from './components/InferenceDisplay';
import InferenceInstruction from './components/InferenceInstruction';
import ConnectionSettings from './components/ConnectionSettings';

const App: React.FC = () => {
  const [host, setHost] = useState('0.0.0.0');
  const [port, setPort] = useState('3000');
  const [key, setKey] = useState(0);

  const baseUrl = `http://${host}:${port}`;
  const wsUrl = `ws://${host}:${port}`;

  const handleConnectionUpdate = (newHost: string, newPort: string) => {
    setHost(newHost);
    setPort(newPort);
    // Increment key to force re-mounting the components
    setKey(prevKey => prevKey + 1);
  };

  return (
    <div className="App">
      <header className="App-header">
        <h2>Bubbaloop</h2>
      </header>

      <ConnectionSettings onUpdate={handleConnectionUpdate} initialHost={host} initialPort={port} />

      <main key={key}>
        <StreamViewerWebsocket
          wsUrl={`${wsUrl}/api/v0/streaming/ws/0`}
          maxHeight="500px"
        />
        <InferenceInstruction
          settingsUrl={`${baseUrl}/api/v0/inference/settings`}
          placeholder="cap en"
          buttonText="Apply"
        />
        <InferenceResultDisplay
          inferenceUrl={`${baseUrl}/api/v0/inference/result`}
          refreshRate={1000}
          width="500px"
          height="500px"
        />
      </main>
    </div>
  );
};

export default App;