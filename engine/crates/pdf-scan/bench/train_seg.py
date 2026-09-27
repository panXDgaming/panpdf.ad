#!/usr/bin/env python3
import struct
import sys
import numpy as np
import torch
from torch import nn
torch.set_num_threads(int(__import__('os').environ.get('THREADS', '10')))

def conv(i, o, k=3):
    return nn.Conv2d(i, o, k, padding=k // 2)

class Net(nn.Module):

    def __init__(self):
        super().__init__()
        self.e1 = nn.ModuleList([conv(3, 8), conv(8, 8)])
        self.e2 = nn.ModuleList([conv(8, 16), conv(16, 16)])
        self.e3 = nn.ModuleList([conv(16, 32), conv(32, 32)])
        self.b = nn.ModuleList([conv(32, 32), conv(32, 32)])
        self.d3 = conv(64, 16)
        self.d2 = conv(32, 8)
        self.d1 = conv(16, 8)
        self.out = conv(8, 1, 1)

    def layers(self):
        return [*self.e1, *self.e2, *self.e3, *self.b, self.d3, self.d2, self.d1, self.out]

    def forward(self, x):
        r = torch.relu
        pool = nn.functional.max_pool2d
        up = lambda t: nn.functional.interpolate(t, scale_factor=2, mode='nearest')
        s1 = r(self.e1[1](r(self.e1[0](x))))
        s2 = r(self.e2[1](r(self.e2[0](pool(s1, 2)))))
        s3 = r(self.e3[1](r(self.e3[0](pool(s2, 2)))))
        b = r(self.b[1](r(self.b[0](pool(s3, 2)))))
        d3 = r(self.d3(torch.cat([up(b), s3], 1)))
        d2 = r(self.d2(torch.cat([up(d3), s2], 1)))
        d1 = r(self.d1(torch.cat([up(d2), s1], 1)))
        return self.out(d1)

def load(path):
    data = np.load(path)
    pictures = torch.from_numpy(data['pictures']).permute(0, 3, 1, 2)
    masks = torch.from_numpy(data['masks']).unsqueeze(1)
    return (pictures, masks)

def floats(pictures, masks):
    return (pictures.float() / 255.0, masks.float())

def loss_of(logits, masks):
    bce = nn.functional.binary_cross_entropy_with_logits(logits, masks)
    p = torch.sigmoid(logits)
    dice = 1 - (2 * (p * masks).sum((1, 2, 3)) + 1) / (p.sum((1, 2, 3)) + masks.sum((1, 2, 3)) + 1)
    return bce + dice.mean()

def iou(logits, masks):
    p = (logits > 0).float()
    inter = (p * masks).sum((1, 2, 3))
    union = (p + masks > 0).float().sum((1, 2, 3))
    return (inter / union.clamp(min=1)).mean().item()

def save(net, out):
    with open(out, 'wb') as f:
        layers = net.layers()
        f.write(b'PSN1')
        f.write(struct.pack('<I', len(layers)))
        for layer in layers:
            w = layer.weight.detach().numpy().astype('<f4')
            o, i, k, _ = w.shape
            f.write(struct.pack('<III', o, i, k))
            f.write(w.tobytes())
            f.write(layer.bias.detach().numpy().astype('<f4').tobytes())

def main():
    train_x, train_y = load(sys.argv[1])
    val_x, val_y = load(sys.argv[2])
    out = sys.argv[3]
    epochs = int(sys.argv[4]) if len(sys.argv) > 4 else 30
    torch.manual_seed(0)
    net = Net()
    print('weights', sum((p.numel() for p in net.parameters())), flush=True)
    optimiser = torch.optim.Adam(net.parameters(), lr=0.002)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimiser, epochs)
    for epoch in range(epochs):
        net.train()
        order = torch.randperm(len(train_x))
        total = 0.0
        for start in range(0, len(order), 16):
            batch = order[start:start + 16]
            x, y = floats(train_x[batch], train_y[batch])
            if torch.rand(1).item() < 0.5:
                x, y = (x.flip(3), y.flip(3))
            x = (x * torch.empty(1).uniform_(0.7, 1.3) + torch.empty(1).uniform_(-0.1, 0.1)).clamp(0, 1)
            loss = loss_of(net(x), y)
            optimiser.zero_grad()
            loss.backward()
            optimiser.step()
            total += loss.item() * len(batch)
        schedule.step()
        net.eval()
        with torch.no_grad():
            logits = torch.cat([net(floats(val_x[i:i + 50], val_y[i:i + 50])[0]) for i in range(0, len(val_x), 50)])
            score = iou(logits, val_y.float())
        print(f'epoch {epoch + 1} loss {total / len(train_x):.4f} val IoU {score:.4f}', flush=True)
        save(net, out)
    save(net, out)
    print('written', out, flush=True)
if __name__ == '__main__':
    main()
