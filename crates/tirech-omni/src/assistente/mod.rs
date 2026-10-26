//! A IA dentro do app (M52): gerar comando, explicar e conversar ao lado do terminal.
//!
//! Os módulos daqui são puros (sem widgets): o contexto que se junta do terminal, os prompts e o
//! que se faz com a resposta. As telas ficam em `tela/assistente.rs`. A IA **nunca executa**
//! nada: a única saída para o terminal é inserir um comando na linha, sem Enter.

pub mod contexto;
pub mod envio;
pub mod perfis;
pub mod prompts;
pub mod resposta;
