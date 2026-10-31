//! As ferramentas do MCP que mexem em arquivos e túneis (M51, seção 7.1): listar, baixar e enviar
//! arquivos pelo SFTP (ou o que a conexão abre), e listar, ligar e desligar túneis. Os arquivos
//! do computador ficam confinados à pasta de troca com o assistente.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tomni_ui::files::{EntryKind, Opener as ArquivosOpener};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{ForwardState, ForwardStatus, SshForwards};

use super::mcp::{Plano, Saida, Sonda, Trabalho};
use super::{Lugar, Tela};
use crate::abrir::forward_do_tunel;
use crate::mcp::permissao::{PermissaoIa, permissao_efetiva};
use crate::modelo::{Connection, TipoTunel, Tunel};
use crate::t;

/// O mais que `baixar_arquivo` sem `destino` devolve como texto.
const LIMITE_DO_TEXTO: u64 = 4 * 1024 * 1024;

/// Quanto tempo `ligar_tunel` espera a aba abrir.
const PRAZO_DA_ABERTURA: Duration = Duration::from_secs(60);

/// A pasta de troca com o assistente quando a configuração não diz: `troca` na pasta de dados no
/// modo portable (`portatil`), senão `Downloads/Tirech Omni`.
fn pasta_padrao(portatil: Option<&Path>) -> PathBuf {
    if let Some(dados) = portatil {
        return dados.join("troca");
    }
    let casa = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    casa.join("Downloads").join("Tirech Omni")
}

/// `caminho` dentro de `pasta`: relativo vale a partir dela; `..` e o que sai da pasta, não. A
/// pasta é criada se não existe quando `criar` (o envio e o download precisam dela).
fn dentro_da_pasta(pasta: &Path, caminho: &str) -> Result<PathBuf, String> {
    let erro = || t!("mcp-fora-da-pasta", pasta = pasta.display().to_string());
    let pedido = Path::new(caminho);
    let completo = if pedido.is_absolute() {
        pedido.to_path_buf()
    } else {
        pasta.join(pedido)
    };
    if completo
        .components()
        .any(|c| matches!(c, Component::ParentDir))
        || !completo.starts_with(pasta)
    {
        return Err(erro());
    }
    // Um link simbólico não leva para fora: o que existe é resolvido e conferido.
    let mut existente = completo.as_path();
    while !existente.exists() {
        match existente.parent() {
            Some(pai) => existente = pai,
            None => return Ok(completo),
        }
    }
    let real = existente.canonicalize().map_err(|_| erro())?;
    let base = pasta
        .ancestors()
        .find(|p| p.exists())
        .and_then(|p| p.canonicalize().ok())
        .ok_or_else(erro)?;
    if real.starts_with(&base) {
        Ok(completo)
    } else {
        Err(erro())
    }
}

fn nome_do_tipo(tipo: TipoTunel) -> &'static str {
    match tipo {
        TipoTunel::Local => "local",
        TipoTunel::Remoto => "remoto",
        TipoTunel::Dinamico => "dinamico",
        TipoTunel::SocketUnix => "socket_unix",
    }
}

fn nome_do_tipo_de_entrada(tipo: EntryKind) -> &'static str {
    match tipo {
        EntryKind::File => "arquivo",
        EntryKind::Dir => "pasta",
        EntryKind::Symlink => "link",
        EntryKind::Other => "outro",
    }
}

/// O estado do túnel `n` da conexão `id` na sessão, se ela já o conhece.
fn status_do_tunel(
    forwards: &SshForwards,
    id: u64,
    n: usize,
    tunel: &Tunel,
) -> Option<ForwardStatus> {
    let procurado = forward_do_tunel(id, n, tunel);
    forwards.list().into_iter().find(|s| s.forward == procurado)
}

fn estado_do_forward(estado: &ForwardState) -> &'static str {
    match estado {
        ForwardState::Starting => "subindo",
        ForwardState::Listening => "escutando",
        ForwardState::Stopped => "parado",
        ForwardState::Failed(_) => "falhou",
    }
}

/// Qual túnel da conexão o assistente quis dizer: o índice (a partir de 1) ou o nome.
fn achar_tunel(conexao: &Connection, valor: &Value) -> Result<usize, String> {
    let numero = match valor {
        Value::Number(n) => n.as_u64().and_then(|n| usize::try_from(n).ok()),
        Value::String(s) => s.trim().parse::<usize>().ok(),
        _ => None,
    };
    if let Some(n) = numero
        && n >= 1
        && n <= conexao.tuneis.len()
    {
        return Ok(n - 1);
    }
    let nome = valor.as_str().unwrap_or_default().trim().to_lowercase();
    let iguais: Vec<usize> = conexao
        .tuneis
        .iter()
        .enumerate()
        .filter(|(_, t)| !nome.is_empty() && t.nome.trim().to_lowercase() == nome)
        .map(|(n, _)| n)
        .collect();
    match iguais.as_slice() {
        [n] => Ok(*n),
        [] => Err(t!(
            "mcp-tunel-nao-existe",
            tunel = valor
                .as_str()
                .map_or_else(|| valor.to_string(), str::to_owned),
            conexao = conexao.name.as_str()
        )),
        _ => Err(t!("mcp-tunel-ambiguo", tunel = nome)),
    }
}

impl Tela {
    fn pasta_de_troca(&self) -> PathBuf {
        let escolhida = self.config.with_untracked(|c| c.ia.pasta_troca.clone());
        let escolhida = escolhida.trim();
        if escolhida.is_empty() {
            pasta_padrao(self.ferramentas.ambiente().portatil.as_deref())
        } else {
            PathBuf::from(escolhida)
        }
    }

    /// Quem abre os arquivos da conexão do plano: pela conexão SSH da aba, se há, senão por uma
    /// conexão própria com a senha do cofre. O texto devolvido é a dica a somar a um erro da
    /// abertura própria (a senha não estar salva).
    fn arquivos_do_plano(&self, plano: &Plano) -> Result<(ArquivosOpener, Option<String>), String> {
        if let Some(sessao) = plano
            .sessao
            .filter(|s| !s.closed() && s.ssh_channels().is_some())
        {
            let abrir = self.opener.arquivos_da_sessao(sessao)?;
            return Ok((abrir, None));
        }
        let conexao = plano
            .conexao
            .as_ref()
            .ok_or_else(|| t!("mcp-conexao-invalida"))?;
        let senhas = self.senhas_do_cofre(conexao);
        let sem_senha = senhas.conexao.is_none() && conexao.key_file.is_none();
        let abrir = self
            .opener
            .arquivos(&self.conexao_efetiva(conexao), senhas)?;
        let dica = sem_senha.then(|| t!("mcp-arquivos-sem-senha", conexao = conexao.name.as_str()));
        Ok((abrir, dica))
    }

    /// `listar_arquivos`.
    pub(super) fn mcp_listar_arquivos(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let Some(caminho) = t.pedido.args["caminho"].as_str().map(str::to_owned) else {
            let erro = t!("mcp-falta-argumento", nome = "caminho");
            return self.terminar_mcp(&t, Saida::Erro(erro));
        };
        let (abrir, dica) = match self.arquivos_do_plano(&plano) {
            Ok(par) => par,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        self.em_thread(
            t,
            Some(Duration::from_secs(60)),
            move || {
                let mut fs = abrir().map_err(|e| com_dica(e.to_string(), &dica))?;
                let mut entradas = fs.list(&caminho).map_err(|e| e.to_string())?;
                entradas.sort_by(|a, b| a.name.cmp(&b.name));
                let lista: Vec<Value> = entradas
                    .iter()
                    .map(|e| {
                        let modificado = e
                            .modified
                            .and_then(|m| jiff::Timestamp::try_from(m).ok())
                            .map(|m| m.to_string());
                        json!({
                            "nome": e.name,
                            "tipo": nome_do_tipo_de_entrada(e.kind),
                            "tamanho": e.size,
                            "modificado": modificado,
                            "permissoes": e.permissions.map(|p| format!("{p:o}")),
                            "destino_do_link": e.link_target,
                        })
                    })
                    .collect();
                Ok::<_, String>(serde_json::to_string_pretty(&lista).unwrap_or_default())
            },
            |_, resultado| match resultado {
                Ok(texto) => Saida::servidor("arquivos", texto, None),
                Err(erro) => Saida::Erro(erro),
            },
        );
    }

    /// `baixar_arquivo`: para a pasta de troca (com `destino`) ou como texto.
    pub(super) fn mcp_baixar_arquivo(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(caminho) = args["caminho"].as_str().map(str::to_owned) else {
            let erro = t!("mcp-falta-argumento", nome = "caminho");
            return self.terminar_mcp(&t, Saida::Erro(erro));
        };
        let destino = match args["destino"].as_str().filter(|d| !d.is_empty()) {
            Some(d) => match dentro_da_pasta(&self.pasta_de_troca(), d) {
                Ok(caminho) => Some(caminho),
                Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
            },
            None => None,
        };
        let (abrir, dica) = match self.arquivos_do_plano(&plano) {
            Ok(par) => par,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        let recebido = destino.clone();
        self.em_thread(
            t,
            None,
            move || {
                let mut fs = abrir().map_err(|e| com_dica(e.to_string(), &dica))?;
                let tamanho = fs.stat(&caminho).map_err(|e| e.to_string())?.size;
                let mut lendo = fs.open_read(&caminho, 0).map_err(|e| e.to_string())?;
                let resultado = match &destino {
                    Some(arquivo) => {
                        if let Some(pasta) = arquivo.parent() {
                            std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
                        }
                        let mut saida =
                            std::fs::File::create(arquivo).map_err(|e| e.to_string())?;
                        let total =
                            std::io::copy(&mut lendo, &mut saida).map_err(|e| e.to_string())?;
                        saida.flush().map_err(|e| e.to_string())?;
                        Baixado::Gravado(total)
                    }
                    None => {
                        if tamanho > LIMITE_DO_TEXTO {
                            return Err(t!("mcp-arquivo-grande", destino = "destino"));
                        }
                        let mut bytes = Vec::new();
                        lendo
                            .by_ref()
                            .take(LIMITE_DO_TEXTO + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        if bytes.contains(&0) {
                            return Err(t!("mcp-arquivo-binario"));
                        }
                        match String::from_utf8(bytes) {
                            Ok(texto) => Baixado::Texto(texto),
                            Err(_) => return Err(t!("mcp-arquivo-binario")),
                        }
                    }
                };
                lendo.finish().map_err(|e| e.to_string())?;
                Ok(resultado)
            },
            move |_, resultado| match resultado {
                Ok(Baixado::Texto(texto)) => Saida::servidor("arquivo", texto, None),
                Ok(Baixado::Gravado(total)) => Saida::App(t!(
                    "mcp-baixou",
                    bytes = total.to_string(),
                    destino = recebido
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                )),
                Err(erro) => Saida::Erro(erro),
            },
        );
    }

    /// `enviar_arquivo`: um arquivo da pasta de troca (ou um que a pessoa soltou ou colou no
    /// app) ou um texto, para o caminho do servidor.
    pub(super) fn mcp_enviar_arquivo(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(caminho) = args["caminho"].as_str().map(str::to_owned) else {
            let erro = t!("mcp-falta-argumento", nome = "caminho");
            return self.terminar_mcp(&t, Saida::Erro(erro));
        };
        let sobrescrever = args["sobrescrever"].as_bool().unwrap_or(false);
        let conteudo = args["conteudo"].as_str().map(str::to_owned);
        let origem = match (&conteudo, args["origem"].as_str().filter(|o| !o.is_empty())) {
            (Some(_), Some(_)) => {
                return self.terminar_mcp(&t, Saida::Erro(t!("mcp-origem-ou-conteudo")));
            }
            (None, None) => {
                return self.terminar_mcp(&t, Saida::Erro(t!("mcp-origem-ou-conteudo")));
            }
            (None, Some(o)) => match dentro_da_pasta(&self.pasta_de_troca(), o) {
                Ok(caminho) => Some(caminho),
                // O que a pessoa soltou ou colou no app também vale.
                Err(erro) => match self.origem_entregue(o) {
                    Some(caminho) => Some(caminho),
                    None => return self.terminar_mcp(&t, Saida::Erro(erro)),
                },
            },
            (Some(_), None) => None,
        };
        let (abrir, dica) = match self.arquivos_do_plano(&plano) {
            Ok(par) => par,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        let destino = caminho.clone();
        self.em_thread(
            t,
            None,
            move || {
                let mut fs = abrir().map_err(|e| com_dica(e.to_string(), &dica))?;
                if !sobrescrever && fs.lstat(&caminho).is_ok() {
                    return Err(t!("mcp-arquivo-ja-existe", caminho = caminho.as_str()));
                }
                let mut gravando = fs.open_write(&caminho, 0).map_err(|e| e.to_string())?;
                let total = match (&origem, &conteudo) {
                    (Some(arquivo), _) => {
                        let mut lendo = std::fs::File::open(arquivo).map_err(|e| e.to_string())?;
                        std::io::copy(&mut lendo, &mut gravando).map_err(|e| e.to_string())?
                    }
                    (None, Some(texto)) => {
                        gravando
                            .write_all(texto.as_bytes())
                            .map_err(|e| e.to_string())?;
                        texto.len() as u64
                    }
                    (None, None) => 0,
                };
                gravando.finish().map_err(|e| e.to_string())?;
                Ok(total)
            },
            move |_, resultado| match resultado {
                Ok(total) => Saida::App(t!(
                    "mcp-enviou",
                    bytes = total.to_string(),
                    caminho = destino.as_str()
                )),
                Err(erro) => Saida::Erro(erro),
            },
        );
    }

    /// `listar_tuneis`: os túneis das conexões liberadas (ou da pedida), com o estado de cada um
    /// quando a conexão tem aba.
    pub(super) fn mcp_listar_tuneis(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let conexoes: Vec<Connection> = match plano.conexao {
            Some(c) => vec![c],
            None => self.tree.with_untracked(|tr| {
                tr.connections()
                    .into_iter()
                    .filter(|c| permissao_efetiva(tr, c.id) != PermissaoIa::Desligada)
                    .cloned()
                    .collect()
            }),
        };
        let mut lista = Vec::new();
        for conexao in conexoes.iter().filter(|c| !c.tuneis.is_empty()) {
            let forwards = self
                .aba_da_conexao(conexao.id)
                .and_then(|(_, s)| s.ssh_forwards());
            for (n, tunel) in conexao.tuneis.iter().enumerate() {
                let status = forwards
                    .as_ref()
                    .and_then(|f| status_do_tunel(f, conexao.id, n, tunel));
                let estado = match (&status, &forwards) {
                    (Some(s), _) => estado_do_forward(&s.state),
                    (None, Some(_)) => "desligado",
                    (None, None) => "sem_aba",
                };
                let mut item = json!({
                    "conexao": conexao.id,
                    "nome_da_conexao": conexao.name,
                    "indice": n + 1,
                    "nome": tunel.nome,
                    "tipo": nome_do_tipo(tunel.tipo),
                    "porta": tunel.porta,
                    "destino": tunel.destino(),
                    "ligado_na_configuracao": tunel.ligado,
                    "estado": estado,
                });
                if let Some(s) = status {
                    item["escuta_em"] = json!(s.listening);
                    if let ForwardState::Failed(motivo) = s.state {
                        item["motivo"] = Value::String(motivo);
                    }
                }
                lista.push(item);
            }
        }
        let texto = serde_json::to_string_pretty(&lista).unwrap_or_else(|_| "[]".to_owned());
        self.terminar_mcp(&t, Saida::App(texto));
    }

    /// A aba de pé com túneis em que o túnel da conexão `conexao` liga: a do pedido (`sessao`)
    /// ou uma da conexão. `None`: ligar abre uma aba nova.
    pub(super) fn aba_com_tuneis(&self, conexao: u64, sessao: Option<Session>) -> Option<Session> {
        let com_tuneis = |s: &Session| !s.closed() && s.ssh_forwards().is_some();
        sessao.filter(com_tuneis).or_else(|| {
            self.aba_da_conexao(conexao)
                .map(|(_, s)| s)
                .filter(com_tuneis)
        })
    }

    /// `ligar_tunel` e `desligar_tunel`. Ligar abre a aba da conexão se não houver.
    pub(super) fn mcp_mexer_no_tunel(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano, ligar: bool) {
        let Some(conexao) = plano.conexao.clone() else {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-conexao-invalida")));
        };
        let n = match achar_tunel(&conexao, &t.pedido.args["tunel"]) {
            Ok(n) => n,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        let tunel = conexao.tuneis[n].clone();
        let existente = self
            .aba_com_tuneis(conexao.id, plano.sessao)
            .and_then(|s| s.ssh_forwards());
        if let Some(forwards) = existente {
            let saida = self.mexer_no_forward(&forwards, conexao.id, n, &tunel, ligar);
            return self.terminar_mcp(&t, saida);
        }
        if !ligar {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-tunel-sem-aba")));
        }
        if tunel.problema().is_some() {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-tunel-invalido")));
        }
        if let Err(erro) = self.pode_abrir_aba(conexao.id, plano.abre_aba) {
            return self.terminar_mcp(&t, Saida::Erro(erro));
        }
        // Sem aba: abre a da conexão e liga quando o SSH subir.
        let antes: Vec<String> = self
            .sessoes_das_janelas(None)
            .iter()
            .map(|(_, s)| s.key_str())
            .collect();
        self.abrir_com(conexao.id, Lugar::default(), true);
        let prefixo = format!("{}-", conexao.id);
        let inicio = Instant::now();
        let mut marcada = false;
        let trabalho = Rc::clone(&t);
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let nova = tela
                    .sessoes_das_janelas(None)
                    .into_iter()
                    .map(|(_, s)| s)
                    .find(|s| {
                        let chave = s.key_str();
                        chave.starts_with(&prefixo) && !antes.contains(&chave)
                    });
                if let Some(sessao) = nova {
                    if !marcada {
                        marcada = true;
                        let chave = sessao.key_str();
                        tela.marcar_aba_mcp(&trabalho, &chave);
                        tela.dar_aba(&chave, trabalho.pedido.ligacao);
                    }
                    if sessao.closed() {
                        return Sonda::Fim(Saida::Erro(t!("mcp-tunel-aba-caiu")));
                    }
                    if let Some(forwards) = sessao.ssh_forwards() {
                        return Sonda::Fim(
                            tela.mexer_no_forward(&forwards, conexao.id, n, &tunel, true),
                        );
                    }
                }
                if inicio.elapsed() >= PRAZO_DA_ABERTURA {
                    return Sonda::Fim(Saida::Erro(t!("mcp-abrir-sem-aba")));
                }
                Sonda::Depois(Duration::from_millis(200))
            }),
        );
    }

    /// Liga ou desliga o túnel `n` nos túneis da sessão.
    fn mexer_no_forward(
        &self,
        forwards: &SshForwards,
        id: u64,
        n: usize,
        tunel: &Tunel,
        ligar: bool,
    ) -> Saida {
        let nome = if tunel.nome.is_empty() {
            (n + 1).to_string()
        } else {
            tunel.nome.clone()
        };
        let status = status_do_tunel(forwards, id, n, tunel);
        if ligar {
            match status {
                Some(s) if matches!(s.state, ForwardState::Listening | ForwardState::Starting) => {}
                Some(s) => forwards.restart(s.id),
                None => {
                    forwards.add(forward_do_tunel(id, n, tunel));
                }
            }
            Saida::App(t!("mcp-tunel-ligado", tunel = nome))
        } else {
            match status {
                Some(s) => {
                    forwards.stop(s.id);
                    Saida::App(t!("mcp-tunel-desligado", tunel = nome))
                }
                None => Saida::App(t!("mcp-tunel-ja-desligado", tunel = nome)),
            }
        }
    }
}

/// O que um download devolveu.
enum Baixado {
    /// Gravado na pasta de troca: quantos bytes.
    Gravado(u64),
    /// O conteúdo como texto.
    Texto(String),
}

/// O erro de abertura com a dica de a senha não estar salva, quando cabe.
fn com_dica(erro: String, dica: &Option<String>) -> String {
    match dica {
        Some(d) => format!("{erro}\n{d}"),
        None => erro,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn dentro_da_pasta_aceita_o_que_esta_dentro() {
        let pasta = std::env::temp_dir().join("troca-teste-a");
        assert_eq!(
            dentro_da_pasta(&pasta, "a/b.txt"),
            Ok(pasta.join("a/b.txt"))
        );
        assert_eq!(
            dentro_da_pasta(&pasta, pasta.join("c.txt").to_str().unwrap_or_default()),
            Ok(pasta.join("c.txt"))
        );
    }

    #[test]
    fn dentro_da_pasta_recusa_o_que_sai() {
        let pasta = std::env::temp_dir().join("troca-teste-b");
        assert!(dentro_da_pasta(&pasta, "../fora.txt").is_err());
        assert!(dentro_da_pasta(&pasta, "a/../../fora.txt").is_err());
        assert!(dentro_da_pasta(&pasta, "/etc/passwd").is_err());
    }

    #[test]
    fn dentro_da_pasta_recusa_link_para_fora() {
        let base = std::env::temp_dir().join(format!("troca-teste-c-{}", std::process::id()));
        let pasta = base.join("troca");
        let fora = base.join("fora");
        std::fs::create_dir_all(&pasta).unwrap();
        std::fs::create_dir_all(&fora).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&fora, pasta.join("atalho")).unwrap();
            assert!(dentro_da_pasta(&pasta, "atalho/segredo.txt").is_err());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn tunel_se_acha_pelo_indice_e_pelo_nome() {
        let mut c = Connection::new("servidor", crate::modelo::Kind::Ssh);
        let mut a = Tunel::novo(TipoTunel::Local, false);
        a.nome = "Banco".to_owned();
        c.tuneis = vec![a, Tunel::novo(TipoTunel::Dinamico, false)];
        assert_eq!(achar_tunel(&c, &json!("banco")), Ok(0));
        assert_eq!(achar_tunel(&c, &json!(2)), Ok(1));
        assert_eq!(achar_tunel(&c, &json!("2")), Ok(1));
        assert!(achar_tunel(&c, &json!("nenhum")).is_err());
        assert!(achar_tunel(&c, &json!(3)).is_err());
    }
}
