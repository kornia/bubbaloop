// StreamViewerWebsocket.tsx
import React, { useState, useEffect, useRef } from 'react';
import './StreamViewerWebsocket.css';

interface StreamViewerWebsocketProps {
    wsUrl: string;
    maxWidth?: string | number;
    maxHeight?: string | number;
    initialWidth?: string | number;
}

const StreamViewerWebsocket: React.FC<StreamViewerWebsocketProps> = ({
    wsUrl,
    maxWidth = '100%',
    maxHeight = '80vh',
    initialWidth = '100%',
}) => {
    const [imageUrl, setImageUrl] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [isConnecting, setIsConnecting] = useState<boolean>(true);
    const [imageStyle, setImageStyle] = useState<React.CSSProperties>({
        maxWidth: '100%',
        maxHeight: '100%',
        objectFit: 'contain',
        display: 'block',
        margin: '0 auto'
    });
    const [containerStyle] = useState<React.CSSProperties>({
        width: initialWidth,
        height: typeof maxHeight === 'string' ? maxHeight : `${maxHeight}px`,
        backgroundColor: '#1a1a1a',
        overflow: 'hidden',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        position: 'relative',
        borderRadius: '8px'
    });

    // Use refs to track current state without triggering renders
    const wsRef = useRef<WebSocket | null>(null);
    const isMountedRef = useRef<boolean>(true);
    const containerRef = useRef<HTMLDivElement>(null);
    const imageRef = useRef<HTMLImageElement>(null);
    const frameBufferRef = useRef<Blob | null>(null);
    const pendingRenderRef = useRef<boolean>(false);
    const lastRenderTimeRef = useRef<number>(0);

    // Keep image dimensions in a ref to avoid re-renders
    const dimensionsRef = useRef<{ width: number, height: number } | null>(null);

    // Performance optimization: limit frame rate
    const targetFps = 24;
    const minFrameInterval = 1000 / targetFps;

    useEffect(() => {
        connectWebSocket();
        isMountedRef.current = true;

        // Use requestAnimationFrame for smoother rendering without changing container size
        const renderLoop = () => {
            const now = performance.now();
            const elapsed = now - lastRenderTimeRef.current;

            if (pendingRenderRef.current && elapsed >= minFrameInterval) {
                if (frameBufferRef.current && isMountedRef.current) {
                    // Revoke previous blob URL
                    if (imageUrl && imageUrl.startsWith('blob:')) {
                        URL.revokeObjectURL(imageUrl);
                    }

                    // Create and set new blob URL
                    const url = URL.createObjectURL(frameBufferRef.current);
                    setImageUrl(url);

                    pendingRenderRef.current = false;
                    lastRenderTimeRef.current = now;
                }
            }

            if (isMountedRef.current) {
                requestAnimationFrame(renderLoop);
            }
        };

        requestAnimationFrame(renderLoop);

        return () => {
            isMountedRef.current = false;
            disconnectWebSocket();

            if (imageUrl && imageUrl.startsWith('blob:')) {
                URL.revokeObjectURL(imageUrl);
            }
        };
    }, [wsUrl]);

    const handleImageLoad = () => {
        if (imageRef.current) {
            const { naturalWidth, naturalHeight } = imageRef.current;
            if (naturalWidth && naturalHeight) {
                dimensionsRef.current = {
                    width: naturalWidth,
                    height: naturalHeight
                };
                // Store dimensions but don't modify any styles
            }
        }
    };

    const connectWebSocket = () => {
        disconnectWebSocket();
        setIsConnecting(true);
        setError(null);

        try {
            const ws = new WebSocket(wsUrl);
            wsRef.current = ws;

            ws.binaryType = 'arraybuffer';

            ws.onopen = () => {
                if (!isMountedRef.current) return;
                console.log('WebSocket connection established');
                setIsConnecting(false);
            };

            ws.onmessage = (event) => {
                if (!isMountedRef.current) return;

                if (typeof event.data === 'string') {
                    setError(event.data);
                    return;
                }

                try {
                    // Store the new frame in the buffer without immediately rendering
                    frameBufferRef.current = new Blob([event.data], { type: 'image/jpeg' });
                    pendingRenderRef.current = true;

                    // Clear any previous errors
                    if (error) setError(null);
                } catch (err) {
                    console.error('Error processing image data:', err);
                }
            };

            ws.onerror = () => {
                if (!isMountedRef.current) return;
                setError('Connection error occurred');
                setIsConnecting(false);
            };

            ws.onclose = (event) => {
                if (!isMountedRef.current) return;

                // Only attempt reconnection if component is still mounted and closure wasn't intentional
                if (isMountedRef.current && event.code !== 1000) {
                    setError('Connection closed. Reconnecting...');
                    // Use exponential backoff for reconnection
                    setTimeout(() => {
                        if (isMountedRef.current) {
                            connectWebSocket();
                        }
                    }, 2000);
                }
            };
        } catch (err) {
            setError('Failed to create WebSocket connection');
            setIsConnecting(false);
        }
    };

    const disconnectWebSocket = () => {
        if (wsRef.current) {
            if (wsRef.current.readyState === WebSocket.OPEN ||
                wsRef.current.readyState === WebSocket.CONNECTING) {
                wsRef.current.close(1000, 'Disconnection requested by client');
            }
            wsRef.current = null;
        }
    };

    return (
        <div
            ref={containerRef}
            className="stream-viewer"
            style={containerStyle}
        >
            <div className="stream-inner-container" style={{
                position: 'relative',
                width: '100%',
                height: '100%',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center'
            }}>
                {imageUrl && (
                    <img
                        ref={imageRef}
                        src={imageUrl}
                        alt="Stream"
                        className="stream-image"
                        onLoad={handleImageLoad}
                        style={imageStyle}
                    />
                )}
                {error && (
                    <div className="stream-error">
                        <p>{error}</p>
                        <button onClick={connectWebSocket} className="retry-button">
                            Reconnect
                        </button>
                    </div>
                )}
                {isConnecting && !imageUrl && (
                    <div className="stream-loading">Connecting to video stream...</div>
                )}
            </div>
        </div>
    );
};

export default StreamViewerWebsocket;