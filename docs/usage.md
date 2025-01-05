# 💊 Usage

### Request stats

```
just whoami 0.0.0.0 3000
```

```json
Result: {
  "arch": "Arm64",
  "desktop_env": "Unknown",
  "device_name": "nvidia-nano",
  "distro": "Ubuntu 18.04.6 LTS",
  "hostname": "nvidia-nano",
  "platform": "Linux",
  "realname": "nvidia",
  "username": "nvidia"
}
```

### Compute mean and std of a local directory of images

You can request to compute the mean and standard deviation over a dataset of images stored in the server.&#x20;

{% hint style="info" %}
Supports only `jpg | jpeg | png`  formats for now.
{% endhint %}

```
just compute-mean-std 0.0.0.0 3000 /path/to/images
```

From server side you should see something like

![compute\_mean\_std-ezgif com-video-to-gif-converter](https://github.com/user-attachments/assets/22b35c6d-2a97-418c-a6f1-dbc131cf5bdb)
