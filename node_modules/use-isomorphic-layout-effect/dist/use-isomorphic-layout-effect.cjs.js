'use strict';

Object.defineProperty(exports, '__esModule', { value: true });

var react = require('react');

var isClient = typeof document !== 'undefined';

var index = isClient ? react.useLayoutEffect : react.useEffect;

exports["default"] = index;
