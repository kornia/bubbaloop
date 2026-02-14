/**
 * dnd-kit test helpers.
 *
 * Wraps components in DndContext + SortableContext so useSortable() works in tests.
 */

import React from 'react';
import { DndContext } from '@dnd-kit/core';
import { SortableContext, rectSortingStrategy } from '@dnd-kit/sortable';

interface DndWrapperOptions {
  items?: string[];
}

/**
 * Wraps a React element with DndContext + SortableContext for testing sortable components.
 */
export function renderWithDndContext(
  ui: React.ReactElement,
  options: DndWrapperOptions = {}
): React.ReactElement {
  const { items = ['item-1'] } = options;
  return React.createElement(
    DndContext,
    null,
    React.createElement(
      SortableContext,
      { items, strategy: rectSortingStrategy, children: ui } as React.Attributes & { items: string[]; strategy: typeof rectSortingStrategy; children: React.ReactElement },
    )
  );
}
