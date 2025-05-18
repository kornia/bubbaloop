import React, { useState } from 'react';
import './ConnectionSettings.css';

interface ConnectionSettingsProps {
    onUpdate: (host: string, port: string) => void;
    initialHost: string;
    initialPort: string;
}

const ConnectionSettings: React.FC<ConnectionSettingsProps> = ({
    onUpdate,
    initialHost,
    initialPort
}) => {
    const [host, setHost] = useState(initialHost);
    const [port, setPort] = useState(initialPort);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        onUpdate(host, port);
    };

    return (
        <div className="connection-settings">
            <form onSubmit={handleSubmit}>
                <div className="form-group">
                    <label htmlFor="host">Host:</label>
                    <input
                        type="text"
                        id="host"
                        value={host}
                        onChange={(e) => setHost(e.target.value)}
                        placeholder="Enter host (e.g., localhost or IP)"
                    />
                </div>
                <div className="form-group">
                    <label htmlFor="port">Port:</label>
                    <input
                        type="text"
                        id="port"
                        value={port}
                        onChange={(e) => setPort(e.target.value)}
                        placeholder="Enter port (e.g., 3000)"
                    />
                </div>
                <button type="submit">Update Connection</button>
            </form>
        </div>
    );
};

export default ConnectionSettings;