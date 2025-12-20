#!/bin/bash
# Set up environment for building with GStreamer

export PKG_CONFIG_PATH="$CONDA_PREFIX/lib/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$CONDA_PREFIX/lib:$LD_LIBRARY_PATH"
export GST_PLUGIN_PATH="$CONDA_PREFIX/lib/gstreamer-1.0:$GST_PLUGIN_PATH"

