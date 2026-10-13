import { filtrarArvore } from "../ui/destino.js";
import { normalizar } from "../ui/texto.js";
import assert from "node:assert/strict";

assert.equal(normalizar("Configuração"), "configuracao");
const arv = [{ id: "a", title: "Redes", children: [{ id: "b", title: "MikroTik", children: [] }] },
             { id: "c", title: "Impressoras", children: [] }];
assert.deepEqual(filtrarArvore(arv, "").length, 2);
assert.deepEqual(filtrarArvore(arv, "mikro").map((n) => n.id), ["a"], "ancestral acompanha");
assert.deepEqual(filtrarArvore(arv, "mikro")[0].children.map((n) => n.id), ["b"]);
assert.deepEqual(filtrarArvore(arv, "REDES").map((n) => n.id), ["a"]);
assert.deepEqual(filtrarArvore([{ id: "x", title: "Configuração", children: [] }], "configuracao").map((n) => n.id), ["x"]);
assert.deepEqual(filtrarArvore(arv, "zzz"), []);
console.log("destino ok");
