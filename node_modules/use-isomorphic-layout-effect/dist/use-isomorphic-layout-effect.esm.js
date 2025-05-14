import { useLayoutEffect, useEffect } from 'react';

var isClient = typeof document !== 'undefined';

var index = isClient ? useLayoutEffect : useEffect;

export { index as default };
