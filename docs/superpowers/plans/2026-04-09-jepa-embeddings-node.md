# I-JEPA Embeddings Node Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Python node that computes I-JEPA embeddings from camera frames and publishes them as JSON on Zenoh.

**Architecture:** Python node using the Bubbaloop Python SDK. Subscribes to `RawImage` (protobuf, RGBA) over local SHM from the RTSP camera node, runs I-JEPA ViT-H/14 inference via HuggingFace `transformers` + PyTorch on CUDA, publishes 1280-dim embedding vectors as JSON on a global Zenoh topic. Drops frames when inference is slower than camera FPS.

**Tech Stack:** Python, Bubbaloop Python SDK, HuggingFace `transformers`, PyTorch (CUDA), Pillow, Zenoh

---

## File Structure

```
jepa-embedder/                    # Standalone repo (separate from bubbaloop)
├── node.yaml                     # Node metadata (role, inputs, outputs, config schema)
├── config.yaml                   # Default runtime config
├── requirements.txt              # Python dependencies
├── src/
│   ├── main.py                   # Entry point: run_node(JepaNode)
│   ├── model.py                  # I-JEPA model loading and inference
│   └── preprocessing.py          # RawImage bytes to torch tensor
└── tests/
    ├── test_preprocessing.py     # Unit tests for image preprocessing
    └── test_model.py             # Unit tests for model wrapper
```

**Responsibilities:**
- `main.py` -- Node lifecycle: subscribe to frames, call model, publish JSON. Owns the frame-dropping logic.
- `model.py` -- Model loading and inference. Isolated so it can be tested without Zenoh.
- `preprocessing.py` -- Converts RGBA bytes + width/height to a normalized 224x224 torch tensor. Pure function, easy to test.

---

### Task 1: Project Scaffold

**Files:**
- Create: `jepa-embedder/node.yaml`
- Create: `jepa-embedder/config.yaml`
- Create: `jepa-embedder/requirements.txt`
- Create: `jepa-embedder/src/__init__.py`
- Create: `jepa-embedder/tests/__init__.py`

- [ ] **Step 1: Create project directory**

```bash
mkdir -p ~/jepa-embedder/src ~/jepa-embedder/tests
cd ~/jepa-embedder
git init
```

- [ ] **Step 2: Create `node.yaml`**

```yaml
apiVersion: bubbaloop.dev/v1
name: jepa-embedder
version: "0.1.0"
type: python
description: "I-JEPA vision embeddings - encodes raw camera frames to embedding vectors"
author: kornia

role: processor

inputs:
  - name: raw
    topic_suffix: "{instance}/input/raw"
    encoding: "application/protobuf;bubbaloop.camera.v1.RawImage"
    description: "RGBA image frames from camera node (SHM)"

outputs:
  - name: embeddings
    topic_suffix: "{instance}/embeddings"
    encoding: "application/json"
    description: "I-JEPA embedding vectors as JSON"

config_schema:
  type: object
  required: [input_topic]
  properties:
    input_topic:
      type: string
      description: "Camera node raw image topic suffix (e.g. rtsp_camera/raw)"
    model:
      type: string
      default: "facebook/ijepa_vith14_1k"
      description: "HuggingFace model identifier"
    device:
      type: string
      enum: ["cuda", "cpu"]
      default: "cuda"
```

- [ ] **Step 3: Create `config.yaml`**

```yaml
name: jepa_front
input_topic: rtsp_camera/raw
model: facebook/ijepa_vith14_1k
device: cuda
```

- [ ] **Step 4: Create `requirements.txt`**

```
torch
transformers
Pillow
bubbaloop-sdk @ git+https://github.com/kornia/bubbaloop.git#subdirectory=python-sdk
```

- [ ] **Step 5: Create empty `__init__.py` files**

```bash
touch src/__init__.py tests/__init__.py
```

- [ ] **Step 6: Commit**

```bash
git add node.yaml config.yaml requirements.txt src/__init__.py tests/__init__.py
git commit -m "feat: scaffold jepa-embedder node"
```

---

### Task 2: Image Preprocessing

**Files:**
- Create: `jepa-embedder/src/preprocessing.py`
- Create: `jepa-embedder/tests/test_preprocessing.py`

- [ ] **Step 1: Write failing test for RGBA-to-tensor conversion**

Create `tests/test_preprocessing.py`:

```python
import torch
from src.preprocessing import preprocess_frame


def test_preprocess_frame_output_shape():
    """224x224 RGB tensor with batch dim from RGBA bytes."""
    width, height = 640, 480
    rgba_bytes = bytes(width * height * 4)  # black RGBA image
    tensor = preprocess_frame(rgba_bytes, width, height)
    assert tensor.shape == (1, 3, 224, 224)
    assert tensor.dtype == torch.float32


def test_preprocess_frame_value_range():
    """Output should be normalized (not 0-255)."""
    width, height = 100, 100
    # All-white RGBA image (255 per channel)
    rgba_bytes = bytes([255] * (width * height * 4))
    tensor = preprocess_frame(rgba_bytes, width, height)
    # After ImageNet normalization, white pixels should NOT be 1.0 or 255.0
    assert tensor.max().item() < 10.0  # normalized values are roughly -2 to +3
    assert tensor.min().item() > -10.0


def test_preprocess_frame_strips_alpha():
    """Alpha channel should be discarded -- output has 3 channels."""
    width, height = 10, 10
    # RGBA: R=100, G=150, B=200, A=255 for every pixel
    pixel = bytes([100, 150, 200, 255])
    rgba_bytes = pixel * (width * height)
    tensor = preprocess_frame(rgba_bytes, width, height)
    assert tensor.shape[1] == 3  # RGB, not RGBA
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd ~/jepa-embedder
python -m pytest tests/test_preprocessing.py -v
```

Expected: FAIL with `ModuleNotFoundError: No module named 'src.preprocessing'`

- [ ] **Step 3: Implement preprocessing**

Create `src/preprocessing.py`:

```python
"""Convert raw RGBA bytes from the camera node to a normalized torch tensor."""

import torch
from PIL import Image
from torchvision import transforms

# ImageNet normalization -- standard for ViT models including I-JEPA
_transform = transforms.Compose([
    transforms.Resize((224, 224)),
    transforms.ToTensor(),  # HWC uint8 [0,255] to CHW float [0,1]
    transforms.Normalize(mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225]),
])


def preprocess_frame(rgba_bytes: bytes, width: int, height: int) -> torch.Tensor:
    """Convert RGBA bytes to a (1, 3, 224, 224) normalized float32 tensor.

    Args:
        rgba_bytes: Raw pixel data in RGBA format (4 bytes per pixel).
        width: Image width in pixels.
        height: Image height in pixels.

    Returns:
        Tensor of shape (1, 3, 224, 224) ready for model input.
    """
    image = Image.frombytes("RGBA", (width, height), rgba_bytes)
    image = image.convert("RGB")
    tensor = _transform(image)
    return tensor.unsqueeze(0)  # add batch dimension
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
python -m pytest tests/test_preprocessing.py -v
```

Expected: 3 passed

- [ ] **Step 5: Commit**

```bash
git add src/preprocessing.py tests/test_preprocessing.py
git commit -m "feat: add image preprocessing (RGBA to normalized tensor)"
```

---

### Task 3: Model Wrapper

**Files:**
- Create: `jepa-embedder/src/model.py`
- Create: `jepa-embedder/tests/test_model.py`

- [ ] **Step 1: Write failing test for model wrapper interface**

Create `tests/test_model.py`:

```python
import torch
from src.model import JepaModel


def test_model_loads_on_cpu():
    """Model should load without errors on CPU."""
    model = JepaModel(model_name="facebook/ijepa_vith14_1k", device="cpu")
    assert model is not None


def test_model_encode_returns_correct_shape():
    """Encode should return a 1D tensor of the model's embedding dim."""
    model = JepaModel(model_name="facebook/ijepa_vith14_1k", device="cpu")
    # Random input tensor matching expected shape
    dummy_input = torch.randn(1, 3, 224, 224)
    embedding = model.encode(dummy_input)
    assert embedding.ndim == 1
    assert embedding.shape[0] == model.embedding_dim


def test_model_encode_is_deterministic():
    """Same input should produce same output (model is in eval mode)."""
    model = JepaModel(model_name="facebook/ijepa_vith14_1k", device="cpu")
    dummy_input = torch.randn(1, 3, 224, 224)
    emb1 = model.encode(dummy_input)
    emb2 = model.encode(dummy_input)
    assert torch.allclose(emb1, emb2)
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
python -m pytest tests/test_model.py -v
```

Expected: FAIL with `ModuleNotFoundError: No module named 'src.model'`

- [ ] **Step 3: Implement model wrapper**

Create `src/model.py`:

```python
"""I-JEPA model wrapper using HuggingFace transformers."""

import logging
import time

import torch
from transformers import AutoModel

log = logging.getLogger(__name__)


class JepaModel:
    """Wraps a HuggingFace I-JEPA model for single-image embedding extraction.

    Args:
        model_name: HuggingFace model identifier (e.g. "facebook/ijepa_vith14_1k").
        device: "cuda" or "cpu".
    """

    def __init__(self, model_name: str, device: str = "cuda"):
        self.device = device
        self.model_name = model_name

        log.info("Loading model %s on %s...", model_name, device)
        t0 = time.monotonic()
        self._model = AutoModel.from_pretrained(model_name)
        self._model.to(device)
        self._model.eval()
        elapsed = time.monotonic() - t0
        log.info("Model loaded in %.1fs", elapsed)

        # ViT-H/14 has hidden_size=1280, ViT-L/16 has 1024, etc.
        self.embedding_dim = self._model.config.hidden_size

    @torch.no_grad()
    def encode(self, pixel_values: torch.Tensor) -> torch.Tensor:
        """Run forward pass and return mean-pooled patch embedding.

        Args:
            pixel_values: Tensor of shape (1, 3, 224, 224), normalized.

        Returns:
            1D tensor of shape (embedding_dim,) -- the mean-pooled patch tokens.
        """
        pixel_values = pixel_values.to(self.device)
        outputs = self._model(pixel_values=pixel_values)
        # I-JEPA has no CLS token -- mean-pool the patch token outputs
        patch_tokens = outputs.last_hidden_state  # (1, num_patches, hidden_size)
        embedding = patch_tokens.mean(dim=1).squeeze(0)  # (hidden_size,)
        return embedding.cpu()
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
python -m pytest tests/test_model.py -v
```

Expected: 3 passed (first run will download the model -- may take a few minutes)

Note: If the model download is too slow or large for your setup, temporarily test with a smaller model by changing the test to use `"facebook/ijepa_vitl16_1k"` (ViT-L/16, around 300M params instead of 630M). The interface is the same.

- [ ] **Step 5: Commit**

```bash
git add src/model.py tests/test_model.py
git commit -m "feat: add I-JEPA model wrapper (HuggingFace transformers)"
```

---

### Task 4: Node Main Loop

**Files:**
- Create: `jepa-embedder/src/main.py`

- [ ] **Step 1: Implement the node**

Create `src/main.py`:

```python
"""I-JEPA embeddings node -- subscribes to camera frames, publishes embeddings as JSON."""

import logging
import time
from datetime import datetime, timezone

from bubbaloop_sdk import run_node, NodeContext

from preprocessing import preprocess_frame
from model import JepaModel

log = logging.getLogger("jepa-embedder")


class JepaNode:
    name = "jepa-embedder"

    def __init__(self, ctx: NodeContext, config: dict):
        self.ctx = ctx
        self.config = config

        device = config.get("device", "cuda")
        model_name = config.get("model", "facebook/ijepa_vith14_1k")
        self.model = JepaModel(model_name=model_name, device=device)

        input_topic = config["input_topic"]
        self.sub = ctx.subscribe(input_topic, local=True)
        self.pub = ctx.publisher_json("embeddings")

        log.info(
            "Ready: input=%s, model=%s, device=%s, dim=%d",
            input_topic, model_name, device, self.model.embedding_dim,
        )

    def run(self):
        while not self.ctx.is_shutdown():
            msg = self.sub.recv()
            if msg is None:
                continue

            t0 = time.monotonic()

            tensor = preprocess_frame(
                rgba_bytes=bytes(msg.data),
                width=msg.width,
                height=msg.height,
            )
            embedding = self.model.encode(tensor)
            inference_ms = (time.monotonic() - t0) * 1000

            self.pub.put({
                "timestamp": datetime.now(timezone.utc).isoformat(),
                "embedding": embedding.tolist(),
                "dim": self.model.embedding_dim,
                "model": self.model.model_name,
                "width": 224,
                "height": 224,
                "inference_ms": round(inference_ms, 1),
            })

            log.debug("Published embedding (%.1f ms)", inference_ms)


if __name__ == "__main__":
    run_node(JepaNode)
```

- [ ] **Step 2: Verify node starts (manual smoke test)**

This requires a running Zenoh router and ideally an RTSP camera node publishing frames. If you don't have a camera running, the node will start and wait for frames.

```bash
cd ~/jepa-embedder

# Terminal 1: ensure zenohd is running
zenohd

# Terminal 2: run the node
python src/main.py -c config.yaml
```

Expected output:
```
2026-04-09T... [INFO] jepa_front -- Starting (type=jepa-embedder, config=config.yaml)
2026-04-09T... [INFO] jepa_front -- Health heartbeat: bubbaloop/global/.../jepa_front/health
2026-04-09T... [INFO] src.model -- Loading model facebook/ijepa_vith14_1k on cuda...
2026-04-09T... [INFO] src.model -- Model loaded in X.Xs
2026-04-09T... [INFO] jepa-embedder -- Ready: input=rtsp_camera/raw, model=facebook/ijepa_vith14_1k, device=cuda, dim=1280
2026-04-09T... [INFO] jepa_front -- Initialized. Running...
```

If a camera is publishing raw frames, you should also see:
```bash
# Terminal 3: verify embeddings are being published
bubbaloop debug subscribe "jepa_front/embeddings"
```

- [ ] **Step 3: Commit**

```bash
git add src/main.py
git commit -m "feat: add node main loop (subscribe, infer, publish JSON)"
```

---

### Task 5: Integration Test and Registration

**Files:**
- Modify: `jepa-embedder/src/main.py` (fix any issues found during testing)

- [ ] **Step 1: Install dependencies**

```bash
cd ~/jepa-embedder
pip install -r requirements.txt
```

If torch is already installed via JetPack (which it should be on your Orin), skip the torch install to avoid overwriting the CUDA-enabled version:

```bash
pip install transformers Pillow
pip install git+https://github.com/kornia/bubbaloop.git#subdirectory=python-sdk
```

- [ ] **Step 2: Run all unit tests**

```bash
cd ~/jepa-embedder
python -m pytest tests/ -v
```

Expected: 6 passed (3 from preprocessing, 3 from model)

- [ ] **Step 3: End-to-end test with camera (if available)**

```bash
# Start the node
python src/main.py -c config.yaml

# In another terminal, subscribe to output
bubbaloop debug subscribe "jepa_front/embeddings"
```

Verify JSON messages appear with `embedding` (list of 1280 floats), `dim`, `model`, and `inference_ms`.

- [ ] **Step 4: Register with bubbaloop daemon**

```bash
bubbaloop node add ~/jepa-embedder
bubbaloop node start jepa-embedder
bubbaloop node logs jepa-embedder -f
```

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration fixes from end-to-end testing"
```

Only commit this if changes were needed. Skip if everything worked.

---

## Verification Checklist

After all tasks, confirm:

1. `python -m pytest tests/ -v` -- 6 tests pass
2. Node starts without errors: `python src/main.py -c config.yaml`
3. Health heartbeat visible: `bubbaloop debug subscribe "jepa_front/health"`
4. Embeddings published as JSON when camera is active: `bubbaloop debug subscribe "jepa_front/embeddings"`
5. Each embedding has 1280 dimensions, `inference_ms` is reasonable (10-100ms on Orin with CUDA)
