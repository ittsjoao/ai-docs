---
name: gerar-manual
description: Gera ou melhora o manual passo a passo de uma sessão do screenManual (pasta atual) e publica o rascunho no Outline. Use com "/gerar-manual gerar" ou "/gerar-manual melhoria".
---

# /gerar-manual

Você está na **pasta de uma sessão** do screenManual. O operador gravou um procedimento real narrando-o. Seu trabalho é transformar a gravação num manual passo a passo, publicar como **rascunho** no Outline e validar o que foi publicado.

## Regras de ferramenta (obrigatórias)

- Leia arquivos só desta pasta (`Read`, `Glob`). Escreva só `steps.json` e `result.json`.
- O shell só pode rodar estes comandos, **um por chamada**, sem `;`, `|`, `&&`, `2>&1` ou qualquer redirecionamento:
  - `screenmanual-cli render`
  - `screenmanual-cli publish`
  - `screenmanual-cli fetch`
  - `screenmanual-cli redact <crops/arquivo.png> <x,y,w,h>` (retângulo em números sem espaços, ex.: `10,20,200,40`; no Windows PowerShell 5.1, `10, 20, 200, 40` vira quatro argumentos)
- O resultado de cada comando vem no stdout e no código de saída:
  - `0` = ok;
  - `1` = erro. Se a mensagem for sobre o `steps.json` ou uma imagem, corrija e repita. Se não for (rede, token ou variável `OUTLINE_*`, Outline inacessível, documento não encontrado), **pare sem escrever `result.json`**: o app reporta o erro;
  - `2` = uso incorreto;
  - `3` = o documento foi editado à mão no Outline. **Pare**, não publique, e escreva `result.json` com `url` e `revision` lidos do `publish.json`, `rodadas` com as correções já feitas, e `validacao` com `{"tipo":"editado_manualmente","detalhe":"<mensagem do CLI>"}`.


## Modo `gerar`

1. **Contexto.** Leia `session.json` (título), `candidates.json` e `transcript.jsonl` (pode estar vazio). O objetivo vem do título e da narração.
2. **Imagens.** Leia primeiro os recortes (`crops/<id>.png`) dos candidatos sem as flags `noise` ou `no_change`, e um print de contexto (`crops/<id>_ctx.png`) por janela. Leia os demais só quando precisar.
3. **Escreva `steps.json`:**
   ```json
   {"schema_version":1,"titulo":"…","objetivo":"…","pre_requisitos":["…"],
    "secoes":[{"titulo":"…","passos":[
      {"candidatos":["c012"],"imagem":"c012","texto":"Preencha **CNPJ** com o CNPJ do cliente, sem pontuação.","aviso":null,"dica":null}]}],
    "descartados":[{"id":"c007","motivo":"clique acidental, desfeito com Esc"}]}
   ```
   - `imagem` é o id de um candidato **com** `crop`, ou `null`.
   - Todo candidato relevante entra em algum passo ou em `descartados`, com o motivo.
4. **Renderize, confira e publique.** Rode `screenmanual-cli render`. Se der erro, corrija o `steps.json` e repita. Com o render ok, **antes do primeiro `publish`**, leia `manual.md` e as imagens em `img/` e confira contra "Dados sensíveis" (abaixo). Se achar algo, rode `screenmanual-cli redact` no recorte em `crops/`, troque a imagem ou ajuste o texto, e rode `render` de novo (o que é publicado fica no Outline, em anexos antigos e no histórico do documento). Só então rode `screenmanual-cli publish`, que devolve `{"url","revision","status"}`.
5. **Valide o publicado.** Rode `screenmanual-cli fetch`. Leia `published/manual.md` e as imagens em `published/img/`, e confira:
   - as imagens estão íntegras e `faltando` e `divergentes` vieram vazios;
   - cada passo condiz com o recorte e com a ação registrada;
   - nenhum candidato relevante ficou de fora, e a ordem está correta;
   - **dados sensíveis** (lista abaixo).

   Para corrigir, use `screenmanual-cli redact` no recorte em `crops/`, troque a imagem ou ajuste o texto. Depois rode `render` e `publish` de novo. **No máximo 2 rodadas de correção.** Se após a 2ª rodada ainda restar algum problema (ex.: dado sensível visível), registre-o em `validacao` com `{"tipo":"pendente","detalhe":"…"}`; o app mostra como aviso do rascunho.
6. **Escreva `result.json`:**
   ```json
   {"url":"…","revision":3,"rodadas":1,"validacao":[{"tipo":"redigido","detalhe":"c014: e-mail do cliente tarjado"}]}
   ```

## Modo `melhoria`

1. Leia o `steps.json` atual e o `feedback.jsonl`. A **última linha** é o pedido atual; as anteriores são só histórico.
2. Aplique **só** o que foi pedido, sem reescrever o resto.
3. Siga os passos 4 a 6 do modo `gerar`.

## Regras de redação

- Português do Brasil, no imperativo, **uma ação por passo**.
- O nome do elemento vai em **negrito**, exatamente como em `el.name` ou como aparece no recorte. **Nunca invente nomes.** Se `el.name` estiver errado ou vazio (apps sem UIA, `quality` `generic`/`none`), leia o rótulo no recorte.
- Valores digitados não existem nos eventos, então descreva-os em vez de copiar: "o CNPJ do cliente".
- A **fala** explica o porquê e o **evento** diz o quê. Em conflito, vale o evento.
- A transcrição erra nomes técnicos. Corrija pelo contexto da tela: títulos de janela e rótulos nos recortes. Exemplo: "inbox" com a janela "WinBox" é **WinBox**.
- Diretivas ditas na fala:
  - "ignora isso" → descarte;
  - "nota:" → `dica`;
  - "atenção:" → `aviso`;
  - pedidos como "remova as gaguejadas" ou "não diga que eu errei" valem para o manual inteiro.
- Separe as seções por janela ou etapa. `aviso` vira `:::warning` e `dica` vira `:::tip` (o `render` cuida disso).

## Dados sensíveis (validar sempre)

Nada destes itens pode aparecer, nem no texto nem nas imagens publicadas:
- CPF, CNPJ de terceiros, e-mails, valores, nomes de clientes;
- **senhas, tokens e usuários de administrador** visíveis em campos ou títulos (ex.: `usuario@10.10.30.1:porta` no título de uma janela);
- **MACs, IPs internos e públicos (IPv4/IPv6)** e listas de equipamentos da rede, além do necessário para o passo. Prefira tarjar as linhas que não fazem parte da ação;
- **termos digitados que aparecem em títulos de janela ou `ancestors`**: uma busca no título do navegador, um comando no título do cmd. Não copie esses títulos para o texto;
- **prints de contexto (`_ctx`)**: o `render` nunca os publica, só os `crop`. Não descreva nem copie para o texto nada que apareça só no fundo deles (gerenciador de senhas, internet banking, e-mail pessoal, janelas fora do procedimento).

Registre cada tarja em `result.json` → `validacao`, com `{"tipo":"redigido","detalhe":"…"}`.
