/**
 * H264 video decoder using WebCodecs API
 *
 * WebCodecs provides hardware-accelerated video decoding directly in the browser.
 * Decodes H264 NAL units frame-by-frame and renders to canvas.
 *
 * Important: This decoder handles Annex B format (with start codes) as received
 * from GStreamer. WebCodecs can decode Annex B directly, but we need to provide
 * SPS/PPS as 'description' in AVCC format for reliable initialization.
 */

export type FrameCallback = (frame: VideoFrame) => void;

export interface H264DecoderOptions {
  onFrame: FrameCallback;
  onError?: (error: Error) => void;
}

interface SPSInfo {
  profileIdc: number;
  constraintFlags: number;
  levelIdc: number;
  spsData: Uint8Array;
  ppsData?: Uint8Array;
}

export class H264Decoder {
  private decoder: VideoDecoder | null = null;
  private options: H264DecoderOptions;
  private initialized = false;
  private waitingForKeyframe = true;
  private spsInfo: SPSInfo | null = null;
  private errorCount = 0;
  private maxErrors = 3;

  constructor(options: H264DecoderOptions) {
    this.options = options;
  }

  /**
   * Check if WebCodecs is supported
   */
  static isSupported(): boolean {
    return typeof VideoDecoder !== 'undefined';
  }

  /**
   * Parse NAL units from Annex B format
   * Returns array of { type, data } for each NAL unit
   */
  private parseNalUnits(data: Uint8Array): Array<{ type: number; data: Uint8Array; offset: number }> {
    const nalUnits: Array<{ type: number; data: Uint8Array; offset: number }> = [];
    let i = 0;

    while (i < data.length - 4) {
      // Find start code (0x00 0x00 0x01 or 0x00 0x00 0x00 0x01)
      if (data[i] === 0 && data[i + 1] === 0) {
        let nalStart = -1;

        if (data[i + 2] === 1) {
          nalStart = i + 3;
        } else if (data[i + 2] === 0 && i + 3 < data.length && data[i + 3] === 1) {
          nalStart = i + 4;
        }

        if (nalStart >= 0 && nalStart < data.length) {
          // Find the end of this NAL (next start code or end of data)
          let nalEnd = data.length;
          for (let j = nalStart; j < data.length - 3; j++) {
            if (data[j] === 0 && data[j + 1] === 0 && (data[j + 2] === 1 || (data[j + 2] === 0 && data[j + 3] === 1))) {
              nalEnd = j;
              break;
            }
          }

          const nalType = data[nalStart] & 0x1f;
          nalUnits.push({
            type: nalType,
            data: data.slice(nalStart, nalEnd),
            offset: i,
          });

          i = nalEnd;
          continue;
        }
      }
      i++;
    }

    return nalUnits;
  }

  /**
   * Extract SPS and PPS from NAL units
   */
  private extractParameterSets(data: Uint8Array): SPSInfo | null {
    const nalUnits = this.parseNalUnits(data);

    let sps: Uint8Array | null = null;
    let pps: Uint8Array | null = null;

    for (const nal of nalUnits) {
      if (nal.type === 7 && !sps) { // SPS
        sps = nal.data;
      } else if (nal.type === 8 && !pps) { // PPS
        pps = nal.data;
      }
    }

    if (sps && sps.length >= 4) {
      return {
        profileIdc: sps[1],
        constraintFlags: sps[2],
        levelIdc: sps[3],
        spsData: sps,
        ppsData: pps || undefined,
      };
    }

    return null;
  }

  /**
   * Initialize the decoder with basic configuration
   */
  async init(): Promise<void> {
    if (!H264Decoder.isSupported()) {
      throw new Error('WebCodecs VideoDecoder not supported');
    }

    this.close();

    this.decoder = new VideoDecoder({
      output: (frame: VideoFrame) => {
        this.errorCount = 0; // Reset error count on successful decode
        this.options.onFrame(frame);
      },
      error: (e: DOMException) => {
        console.error('[H264Decoder] Decoder error:', e.name, e.message);
        this.errorCount++;

        if (this.errorCount >= this.maxErrors) {
          console.error('[H264Decoder] Too many errors, stopping');
          this.options.onError?.(new Error(`${e.name}: ${e.message}`));
        } else {
          // Try to recover by waiting for next keyframe
          console.log('[H264Decoder] Attempting recovery, waiting for next keyframe');
          this.waitingForKeyframe = true;
        }
      },
    });

    this.initialized = true;
    this.waitingForKeyframe = true;
    this.spsInfo = null;
    this.errorCount = 0;

    console.log('[H264Decoder] Decoder created, waiting for SPS/PPS');
  }

  /**
   * Configure decoder with detected codec parameters
   *
   * Important: We do NOT provide the AVCC description because our data is in Annex B format.
   * When description is provided, WebCodecs expects AVCC format (NAL length prefixes).
   * When description is omitted, WebCodecs expects Annex B format (start codes).
   */
  private async configureDecoder(spsInfo: SPSInfo): Promise<boolean> {
    if (!this.decoder || this.decoder.state === 'closed') {
      return false;
    }

    const codec = `avc1.${spsInfo.profileIdc.toString(16).padStart(2, '0')}${spsInfo.constraintFlags.toString(16).padStart(2, '0')}${spsInfo.levelIdc.toString(16).padStart(2, '0')}`;
    console.log(`[H264Decoder] Configuring with codec: ${codec}`);

    // Configure WITHOUT description for Annex B format data
    // When description is omitted, WebCodecs expects Annex B (start codes)
    // When description is provided, WebCodecs expects AVCC (length prefixes)
    const config: VideoDecoderConfig = {
      codec,
      optimizeForLatency: true,
    };

    try {
      const support = await VideoDecoder.isConfigSupported(config);
      if (support.supported) {
        // Reset decoder if it's already configured
        if (this.decoder.state === 'configured') {
          this.decoder.reset();
        }
        this.decoder.configure(config);
        console.log(`[H264Decoder] Configured for Annex B format (no description)`);
        return true;
      } else {
        console.error(`[H264Decoder] Codec ${codec} not supported`);
      }
    } catch (e) {
      console.error('[H264Decoder] Configuration error:', e);
    }

    return false;
  }

  /**
   * Check if data contains a keyframe (IDR NAL unit)
   */
  private containsKeyframe(data: Uint8Array): boolean {
    const nalUnits = this.parseNalUnits(data);
    return nalUnits.some(nal => nal.type === 5); // IDR slice
  }

  /**
   * Decode H264 data
   * @param data - H264 NAL units in Annex B format
   * @param timestamp - Timestamp in microseconds
   */
  async decode(data: Uint8Array, timestamp: number): Promise<void> {
    if (!this.decoder || !this.initialized) {
      return;
    }

    // Check if decoder is in a bad state
    if (this.decoder.state === 'closed') {
      console.log('[H264Decoder] Decoder closed, reinitializing');
      await this.init();
      return;
    }

    const isKeyframe = this.containsKeyframe(data);

    // Wait for a keyframe to start decoding
    if (this.waitingForKeyframe) {
      if (!isKeyframe) {
        return;
      }

      console.log(`[H264Decoder] Keyframe received (${data.length} bytes)`);

      // Extract SPS/PPS and configure decoder
      const spsInfo = this.extractParameterSets(data);
      if (spsInfo) {
        this.spsInfo = spsInfo;

        // Log NAL types for debugging
        const nalUnits = this.parseNalUnits(data);
        const nalTypes = nalUnits.map(n => n.type);
        console.log(`[H264Decoder] NAL types in keyframe: ${nalTypes.join(', ')} (7=SPS, 8=PPS, 5=IDR, 9=AUD, 6=SEI)`);

        const configured = await this.configureDecoder(spsInfo);
        if (!configured) {
          console.error('[H264Decoder] Failed to configure decoder');
          return;
        }
      } else {
        console.warn('[H264Decoder] No SPS found in keyframe, cannot configure decoder');
        return;
      }

      this.waitingForKeyframe = false;
    }

    // Verify decoder is configured
    if (this.decoder.state !== 'configured') {
      console.warn('[H264Decoder] Decoder not configured, waiting for keyframe');
      this.waitingForKeyframe = true;
      return;
    }

    try {
      const chunk = new EncodedVideoChunk({
        type: isKeyframe ? 'key' : 'delta',
        timestamp,
        data,
      });

      this.decoder.decode(chunk);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      console.error('[H264Decoder] EncodedVideoChunk/decode error:', errorMsg);

      // Log data info for debugging on first error
      if (this.errorCount === 0) {
        console.error('[H264Decoder] Failing frame info:', {
          isKeyframe,
          timestamp,
          dataLength: data.length,
          first16Bytes: Array.from(data.slice(0, 16)).map(b => b.toString(16).padStart(2, '0')).join(' '),
        });
      }

      this.errorCount++;

      // On error, wait for next keyframe to resync
      if (this.errorCount >= 2) {
        this.waitingForKeyframe = true;
      }
    }
  }

  /**
   * Flush pending frames
   */
  async flush(): Promise<void> {
    if (this.decoder?.state === 'configured') {
      try {
        await this.decoder.flush();
      } catch (e) {
        console.warn('[H264Decoder] Flush error:', e);
      }
    }
  }

  /**
   * Reset the decoder (call after errors or stream discontinuity)
   */
  async reset(): Promise<void> {
    this.waitingForKeyframe = true;
    this.errorCount = 0;

    if (this.decoder?.state === 'configured') {
      try {
        this.decoder.reset();
      } catch (e) {
        console.warn('[H264Decoder] Reset error:', e);
      }
    }

    // Re-configure if we have SPS info
    if (this.spsInfo && this.decoder?.state === 'unconfigured') {
      await this.configureDecoder(this.spsInfo);
    }
  }

  /**
   * Close the decoder
   */
  close(): void {
    if (this.decoder && this.decoder.state !== 'closed') {
      try {
        this.decoder.close();
      } catch (e) {
        console.warn('[H264Decoder] Close error:', e);
      }
    }
    this.decoder = null;
    this.initialized = false;
    this.waitingForKeyframe = true;
    this.spsInfo = null;
    this.errorCount = 0;
  }

  get isInitialized(): boolean {
    return this.initialized && this.decoder?.state === 'configured';
  }
}
