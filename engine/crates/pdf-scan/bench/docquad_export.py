#!/usr/bin/env python3
import struct
import sys
import numpy as np
from onnxruntime.tools.ort_format_model.ort_flatbuffers_py.fbs import AttributeType as AT
from onnxruntime.tools.ort_format_model.ort_flatbuffers_py.fbs import InferenceSession as IS

def attribute(a):
    t = a.Type()
    if t == AT.AttributeType.INT:
        return a.I()
    if t == AT.AttributeType.INTS:
        return [a.Ints(k) for k in range(a.IntsLength())]
    if t == AT.AttributeType.FLOAT:
        return a.F()
    if t == AT.AttributeType.FLOATS:
        return [a.Floats(k) for k in range(a.FloatsLength())]
    if t == AT.AttributeType.STRING:
        return a.S().decode()
    raise ValueError(t)

def main():
    source, out = (sys.argv[1], sys.argv[2])
    graph = IS.InferenceSession.GetRootAs(open(source, 'rb').read(), 0).Model().Graph()
    kinds = {1: np.float32, 7: np.int64}
    tensors = {}
    for i in range(graph.InitializersLength()):
        t = graph.Initializers(i)
        dims = [t.Dims(j) for j in range(t.DimsLength())]
        tensors[t.Name().decode()] = np.frombuffer(bytes(t.RawDataAsNumpy()), kinds[t.DataType()]).reshape(dims)
    plain = []
    for i in range(graph.NodesLength()):
        n = graph.Nodes(i)
        op = n.OpType().decode()
        ins = [n.Inputs(j).decode() for j in range(n.InputsLength())]
        outs = [n.Outputs(j).decode() for j in range(n.OutputsLength())]
        attrs = {n.Attributes(j).Name().decode(): attribute(n.Attributes(j)) for j in range(n.AttributesLength())}
        if op == 'FusedConv':
            act = attrs.pop('activation', None)
            params = attrs.pop('activation_params', None)
            value = outs[0] + '#conv'
            plain.append(('Conv', ins[:3], value, attrs))
            if len(ins) > 3:
                summed = outs[0] + '#sum' if act else outs[0]
                plain.append(('Add', [value, ins[3]], summed, {}))
                value = summed
            if act == 'Relu':
                plain.append(('Relu', [value], outs[0], {}))
            elif act == 'HardSigmoid':
                alpha, beta = params if params else (0.2, 0.5)
                plain.append(('HardSigmoid', [value], outs[0], {'alpha': alpha, 'beta': beta}))
            elif act is not None:
                raise ValueError(act)
        else:
            plain.append((op, ins, outs[0], attrs))
    known = set(tensors) | {'input', ''}
    ordered = []
    while plain:
        ready = [p for p in plain if all((i in known for i in p[1]))]
        assert ready, 'the graph has a cycle'
        for p in ready:
            ordered.append(p)
            known.add(p[2])
            plain.remove(p)
    slots = {'input': 0}

    def slot(name):
        if name not in slots:
            slots[name] = len(slots)
        return slots[name]
    body = b''
    codes = {'Conv': 1, 'Relu': 2, 'HardSigmoid': 3, 'Mul': 4, 'Add': 5, 'GlobalAveragePool': 6, 'Resize': 7}
    for op, ins, output, attrs in ordered:
        code = codes[op]
        if op == 'Conv':
            w = tensors[ins[1]]
            b = tensors[ins[2]] if len(ins) > 2 and ins[2] else np.zeros(w.shape[0], np.float32)
            o, cg, kh, kw = w.shape
            assert kh == kw and attrs.get('dilations', [1, 1]) == [1, 1]
            stride = attrs.get('strides', [1, 1])
            pads = attrs.get('pads', [0, 0, 0, 0])
            assert stride[0] == stride[1] and len(set(pads)) == 1
            body += struct.pack('<BIIHHBBBH', code, slot(ins[0]), slot(output), o, cg, kh, stride[0], pads[0], attrs.get('group', 1))
            body += w.astype('<f2').tobytes() + b.astype('<f2').tobytes()
        elif op == 'HardSigmoid':
            body += struct.pack('<BIIff', code, slot(ins[0]), slot(output), attrs.get('alpha', 0.2), attrs.get('beta', 0.5))
        elif op in ('Relu', 'GlobalAveragePool'):
            body += struct.pack('<BII', code, slot(ins[0]), slot(output))
        elif op in ('Mul', 'Add'):
            body += struct.pack('<BIII', code, slot(ins[0]), slot(ins[1]), slot(output))
        elif op == 'Resize':
            assert attrs.get('mode') == 'linear' and attrs.get('coordinate_transformation_mode') == 'half_pixel'
            size = tensors[ins[3]]
            body += struct.pack('<BIIHH', code, slot(ins[0]), slot(output), int(size[2]), int(size[3]))
        else:
            raise ValueError(op)
    head = b'PDQ1' + struct.pack('<IIIII', len(slots), slots['input'], slots['corner_heatmaps'], slots['mask_logits'], len(ordered))
    open(out, 'wb').write(head + body)
    print(out, len(head) + len(body), 'bytes,', len(ordered), 'nodes,', len(slots), 'slots')
if __name__ == '__main__':
    main()
