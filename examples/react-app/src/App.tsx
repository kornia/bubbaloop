// App.tsx
import React from 'react';
import './App.css';
import StreamViewerWebsocket from './components/StreamViewerWebsocket';
import InferenceResultDisplay from './components/InferenceDisplay';
import InferenceInstruction from './components/InferenceInstruction';

const App: React.FC = () => {
  return (
    <div className="App">
      <header className="App-header">
        <h2>Bubbaloop</h2>
      </header>

      <main>
        <StreamViewerWebsocket
          wsUrl="ws://0.0.0.0:3000/api/v0/streaming/ws/0"
          maxHeight="500px"
        />
        <InferenceInstruction
          settingsUrl="http://0.0.0.0:3000/api/v0/inference/settings"
          placeholder="cap en"
          buttonText="Apply"
        />
        <InferenceResultDisplay
          inferenceUrl="http://0.0.0.0:3000/api/v0/inference/result"
          refreshRate={1000}
          width="500px"
          height="500px"
        />
      </main>
    </div>
  );
};

export default App;