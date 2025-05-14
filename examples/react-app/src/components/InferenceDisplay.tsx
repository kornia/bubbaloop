// InferenceDisplay.tsx
import React, { useState, useEffect, useRef } from 'react';
import { JSONTree } from 'react-json-tree';
import './InferenceDisplay.css';

interface InferenceDisplayProps {
    inferenceUrl: string;
    refreshRate?: number;
    width?: string | number;
    height?: string | number;
    title?: string;
}

// Monokai theme for direct application
const monokai = {
    scheme: 'monokai',
    base00: '#272822', // background
    base01: '#383830',
    base02: '#49483e',
    base03: '#75715e', // comments
    base04: '#a59f85',
    base05: '#f8f8f2', // text
    base06: '#f5f4f1',
    base07: '#f9f8f5',
    base08: '#f92672', // red
    base09: '#fd971f', // orange
    base0A: '#f4bf75', // yellow
    base0B: '#a6e22e', // green
    base0C: '#a1efe4', // aqua
    base0D: '#66d9ef', // blue
    base0E: '#ae81ff', // purple
    base0F: '#cc6633'  // brown
};

const InferenceDisplay: React.FC<InferenceDisplayProps> = ({
    inferenceUrl,
    refreshRate = 1000,
    width = '100%',
    height = 'auto',
    title = 'Inference Results',
}) => {
    const [inferenceData, setInferenceData] = useState<any>(null);
    const [error, setError] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState<boolean>(true);

    const intervalRef = useRef<number | null>(null);
    const isMountedRef = useRef<boolean>(true);

    useEffect(() => {
        startFetching();
        isMountedRef.current = true;

        return () => {
            isMountedRef.current = false;
            stopFetching();
        };
    }, [inferenceUrl]);

    const startFetching = (): void => {
        stopFetching();
        setIsLoading(true);
        fetchInferenceData();
        intervalRef.current = window.setInterval(fetchInferenceData, refreshRate);
    };

    const stopFetching = (): void => {
        if (intervalRef.current) {
            clearInterval(intervalRef.current);
            intervalRef.current = null;
        }
    };

    const fetchInferenceData = async (): Promise<void> => {
        try {
            const response = await fetch(inferenceUrl, {
                cache: 'no-store',
            });

            if (!isMountedRef.current) return;

            if (!response.ok) {
                throw new Error(`HTTP error! Status: ${response.status}`);
            }

            const jsonData = await response.json();
            setInferenceData(jsonData);
            setError(null);
            setIsLoading(false);
        } catch (err) {
            if (isMountedRef.current) {
                const message = err instanceof Error ? err.message : 'Failed to load inference data';
                setError(message);
                setIsLoading(false);
                console.error('Inference data fetch error:', message);
            }
        }
    };

    // Render error or loading states conditionally
    if (error) {
        return (
            <div className="inference-error">
                <p>{error}</p>
                <button onClick={startFetching} className="retry-button">Retry</button>
            </div>
        );
    }

    if (isLoading && !inferenceData) {
        return <div className="inference-loading">Loading...</div>;
    }

    if (!inferenceData) {
        return <div className="inference-empty">No data available</div>;
    }

    return (
        <div>
            <h2>Inference Response</h2>
            <JSONTree
                data={inferenceData}
                theme={monokai}
                invertTheme={false}
                hideRoot={false}
                shouldExpandNodeInitially={() => true}
            />
        </div>
    );
};

export default InferenceDisplay;