/**
 * Custom render utility that wraps components with all required providers.
 *
 * Mirrors the production provider hierarchy:
 *   FleetProvider > ZenohSubscriptionProvider
 */

import React from 'react';
import { render, type RenderOptions } from '@testing-library/react';
import { FleetProvider } from '../contexts/FleetContext';
import { ZenohSubscriptionProvider } from '../contexts/ZenohSubscriptionContext';

interface CustomRenderOptions extends Omit<RenderOptions, 'wrapper'> {
  /** Mock Zenoh session (null = disconnected) */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  session?: any;
}

/**
 * Renders a component wrapped with FleetProvider and ZenohSubscriptionProvider
 * — matching the production provider hierarchy.
 */
export function renderWithProviders(
  ui: React.ReactElement,
  options: CustomRenderOptions = {}
) {
  const { session = null, ...renderOptions } = options;

  function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <FleetProvider>
        <ZenohSubscriptionProvider session={session}>
          {children}
        </ZenohSubscriptionProvider>
      </FleetProvider>
    );
  }

  return render(ui, { wrapper: Wrapper, ...renderOptions });
}

// Re-export everything from testing-library
export { screen, within, waitFor, act, fireEvent } from '@testing-library/react';
export { default as userEvent } from '@testing-library/user-event';
