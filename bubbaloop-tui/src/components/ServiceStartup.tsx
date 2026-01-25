import React, { useState, useEffect } from 'react';
import { Box, Text } from 'ink';
import Spinner from 'ink-spinner';
import {
  checkServices,
  startServices,
  waitForWebSocket,
  ServiceStatus,
} from '../utils/serviceCheck.js';

type StartupPhase = 'checking' | 'starting' | 'waiting' | 'ready' | 'error';

interface ServiceStartupProps {
  onReady: () => void;
  onError: (error: string) => void;
}

const ServiceIcon: React.FC<{ running: boolean; name: string }> = ({ running, name }) => (
  <Box>
    <Text color={running ? '#95E1D3' : '#FF6B6B'}>{running ? '●' : '○'}</Text>
    <Text color="#888"> {name}</Text>
  </Box>
);

export const ServiceStartup: React.FC<ServiceStartupProps> = ({ onReady, onError }) => {
  const [phase, setPhase] = useState<StartupPhase>('checking');
  const [services, setServices] = useState<ServiceStatus | null>(null);
  const [message, setMessage] = useState('Checking services...');

  useEffect(() => {
    let mounted = true;

    const init = async () => {
      try {
        // Check current service status
        const status = await checkServices();
        if (!mounted) return;
        setServices(status);

        // If services are running, check WebSocket
        if (status.bridge && status.zenohd) {
          setMessage('Checking WebSocket connection...');
          const wsReady = await waitForWebSocket(10001, '127.0.0.1', 3, 500);

          if (wsReady) {
            setPhase('ready');
            onReady();
            return;
          }
        }

        // Need to start services
        setPhase('starting');
        setMessage('Starting services...');

        await startServices();
        if (!mounted) return;

        // Update status after starting
        const newStatus = await checkServices();
        setServices(newStatus);

        setPhase('waiting');
        setMessage('Waiting for services to be ready...');

        // Wait for WebSocket
        const ready = await waitForWebSocket(10001, '127.0.0.1', 10, 1000);

        if (!mounted) return;

        if (ready) {
          setPhase('ready');
          onReady();
        } else {
          throw new Error('Services started but WebSocket not ready on port 10001');
        }
      } catch (error) {
        if (!mounted) return;
        setPhase('error');
        const msg = error instanceof Error ? error.message : String(error);
        setMessage(msg);
        onError(msg);
      }
    };

    init();

    return () => {
      mounted = false;
    };
  }, [onReady, onError]);

  return (
    <Box flexDirection="column" padding={1}>
      <Box marginBottom={1}>
        <Text bold color="#4ECDC4">Bubbaloop Service Status</Text>
      </Box>

      {services && (
        <Box flexDirection="column" marginBottom={1} marginLeft={1}>
          <ServiceIcon running={services.zenohd} name="zenohd (router)" />
          <ServiceIcon running={services.bridge} name="bridge (WebSocket)" />
          <ServiceIcon running={services.daemon} name="daemon" />
        </Box>
      )}

      <Box marginLeft={1}>
        {(phase === 'checking' || phase === 'starting' || phase === 'waiting') && (
          <>
            <Text color="#FFD93D"><Spinner type="dots" /></Text>
            <Text color="#888"> {message}</Text>
          </>
        )}
        {phase === 'ready' && (
          <Text color="#95E1D3">Services ready!</Text>
        )}
        {phase === 'error' && (
          <Box flexDirection="column">
            <Text color="#FF6B6B">Error: {message}</Text>
            <Text color="#888" dimColor>
              Try running: systemctl --user status bubbaloop-bridge
            </Text>
          </Box>
        )}
      </Box>
    </Box>
  );
};

export default ServiceStartup;
