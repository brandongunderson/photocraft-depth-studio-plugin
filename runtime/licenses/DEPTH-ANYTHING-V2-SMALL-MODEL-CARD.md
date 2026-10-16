---
license: apache-2.0
language:
- en
pipeline_tag: depth-estimation
library_name: depth-anything-v2
tags:
- depth
- relative depth
---

# Depth-Anything-V2-Small

## Introduction
Depth Anything V2 is trained from 595K synthetic labeled images and 62M+ real unlabeled images, providing the most capable monocular depth estimation (MDE) model with the following features:
- more fine-grained details than Depth Anything V1
- more robust than Depth Anything V1 and SD-based models (e.g., Marigold, Geowizard)
- more efficient (10x faster) and more lightweight than SD-based models
- impressive fine-tuned performance with our pre-trained models

## Installation

```bash
git clone https://huggingface.co/spaces/depth-anything/Depth-Anything-V2
cd Depth-Anything-V2
pip install -r requirements.txt
```

## Usage

Download the [model](https://huggingface.co/depth-anything/Depth-Anything-V2-Small/resolve/main/depth_anything_v2_vits.pth?download=true) first and put it under the `checkpoints` directory.

```python
import cv2
import torch

from depth_anything_v2.dpt import DepthAnythingV2

model = DepthAnythingV2(encoder='vits', features=64, out_channels=[48, 96, 192, 384])
model.load_state_dict(torch.load('checkpoints/depth_anything_v2_vits.pth', map_location='cpu'))
model.eval()

raw_img = cv2.imread('your/image/path')
depth = model.infer_image(raw_img) # HxW raw depth map
```

## Citation

If you find this project useful, please consider citing:

```bibtex
@article{depth_anything_v2,
  title={Depth Anything V2},
  author={Yang, Lihe and Kang, Bingyi and Huang, Zilong and Zhao, Zhen and Xu, Xiaogang and Feng, Jiashi and Zhao, Hengshuang},
  journal={arXiv:2406.09414},
  year={2024}
}

@inproceedings{depth_anything_v1,
  title={Depth Anything: Unleashing the Power of Large-Scale Unlabeled Data}, 
  author={Yang, Lihe and Kang, Bingyi and Huang, Zilong and Xu, Xiaogang and Feng, Jiashi and Zhao, Hengshuang},
  booktitle={CVPR},
  year={2024}
}