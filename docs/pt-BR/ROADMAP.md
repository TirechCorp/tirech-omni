# Planos

**Português** · [English](../en/ROADMAP.md)

O que está planejado, sem data e sem ordem de prioridade. Um item aqui não é promessa: se
quiser ajudar em algum, abra uma issue antes para combinar a abordagem
([CONTRIBUTING](CONTRIBUTING.md)).

## Qualquer programa do shell local pela VPN

Hoje o shell local sai pela rede do cliente só para quem usa proxy (as variáveis `ALL_PROXY` e
`HTTP_PROXY`) ou por um túnel local. Para valer para qualquer programa, sem configurar nada, a
aba precisaria da própria pilha de rede: um *network namespace* com uma placa `tun` ligada à VPN
embutida (no Linux), ou um adaptador virtual por aba no Windows e no macOS. Isso pede
privilégios (`CAP_NET_ADMIN`, um ajudante instalado com permissão ou um driver), que o app hoje
não precisa, e por isso fica para algum dia.

## Interface em mais idiomas

Hoje a interface está em português (Brasil), inglês e espanhol. Para chegar à maioria dos
idiomas:

- **Idiomas definidos pelos dados.** Descobrir os idiomas pelas pastas de `i18n/`, em vez de
  uma variante no código por idioma, e escolher o do sistema com uma cadeia de reserva
  (`es-MX` → `es` → `en`), com o `fluent-langneg`. Acrescentar um idioma passa a ser só
  traduzir os `.ftl`.
- **Fontes para outras escritas** (na [Tomni UI](https://github.com/TirechCorp/tomni-ui)). A
  Selawik cobre só o alfabeto latino; chinês, japonês e coreano já existem pela feature `cjk`
  (cerca de 4,5 MB). Árabe, hebraico, devanágari, tailandês e outras pedem fontes Noto
  embutidas ou o uso das fontes do sistema.
- **Escrita da direita para a esquerda** (na Tomni UI). Espelhar o layout dos widgets para
  árabe e hebraico; a ordem do texto (bidi) o parley já resolve.
- **Formatos de número, data e tamanho** pelo ICU4X, no lugar das regras do `formatos.ftl`.
  Os plurais o Fluent já resolve.
- **Textos que crescem** (alemão, por exemplo): conferir cada idioma novo com o lint de texto
  cortado e com os snapshots.
- **Busca e quebra de linha** em línguas sem espaço entre as palavras. A entrada por IME e os
  caracteres de largura dupla no terminal já existem.
- **Processo de tradução:** uma plataforma que lê `.ftl` (Weblate, Pontoon ou Crowdin), com os
  testes de pseudotradução que já existem.
- **Textos da biblioteca:** conjuntos prontos das `Strings` da Tomni UI por idioma, em vez de o
  app preencher cada texto.

O texto pesa pouco no binário (cerca de 250 KB por idioma); o que pesa são as fontes.
