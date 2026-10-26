//! Os provedores de IA (M52): o perfil, os presets e o `ia.json`.
//!
//! Um perfil diz **onde** está o modelo (a URL e o dialeto), **qual** é e **como** chegar lá (a
//! rede, o TLS e os cabeçalhos). **A chave nunca entra no JSON**: o perfil guarda só o id do item
//! do cofre onde ela está; quem monta o pedido a lê de lá e a passa para
//! [`PerfilIa::endpoint`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tomni_ui::ai::{Dialect, Endpoint, Locality, Tls, Zeroizing, locality};
use tomni_ui::terminal::Proxy;

use crate::armazenamento::Armazenamento;
use crate::redes::Rede;
use crate::t;

/// A versão do formato do `ia.json`.
pub const VERSAO: u32 = 1;

/// O formato do pedido (espelho serializável do [`Dialect`] da lib).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialeto {
    /// Compatível com OpenAI (Ollama, LM Studio, OpenRouter, OpenAI, Groq, vLLM, llama.cpp…).
    #[default]
    OpenAi,
    /// A API Messages da Anthropic.
    Anthropic,
}

impl Dialeto {
    /// Os dois, na ordem da lista.
    pub const TODOS: [Dialeto; 2] = [Dialeto::OpenAi, Dialeto::Anthropic];

    /// O dialeto da lib.
    pub fn da_lib(self) -> Dialect {
        match self {
            Dialeto::OpenAi => Dialect::OpenAi,
            Dialeto::Anthropic => Dialect::Anthropic,
        }
    }

    /// O nome na lista.
    pub fn nome(self) -> String {
        match self {
            Dialeto::OpenAi => t!("ia-dialeto-openai"),
            Dialeto::Anthropic => t!("ia-dialeto-anthropic"),
        }
    }
}

/// Como o certificado do provedor é conferido.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsIa {
    /// Pelas raízes do sistema.
    #[default]
    Sistema,
    /// Pelas raízes do sistema e mais as autoridades do arquivo PEM neste caminho.
    CaPropria(String),
    /// Aceita qualquer certificado (um servidor de laboratório).
    NaoVerificar,
}

impl TlsIa {
    /// O TLS da lib.
    pub fn da_lib(&self) -> Tls {
        match self {
            TlsIa::Sistema => Tls::System,
            TlsIa::CaPropria(caminho) => Tls::CustomCa(PathBuf::from(caminho)),
            TlsIa::NaoVerificar => Tls::NoVerify,
        }
    }
}

/// Os provedores que o app já sabe preencher.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Ollama neste computador.
    Ollama,
    /// LM Studio neste computador.
    LmStudio,
    /// OpenRouter.
    OpenRouter,
    /// OpenAI.
    OpenAi,
    /// Anthropic.
    Anthropic,
    /// Groq.
    Groq,
    /// Qualquer servidor compatível.
    #[default]
    Personalizado,
}

impl Preset {
    /// Os presets na ordem em que aparecem (o que roda no computador vem primeiro).
    pub const TODOS: [Preset; 7] = [
        Preset::Ollama,
        Preset::LmStudio,
        Preset::OpenRouter,
        Preset::OpenAi,
        Preset::Anthropic,
        Preset::Groq,
        Preset::Personalizado,
    ];

    /// O dialeto que o preset usa.
    pub fn dialeto(self) -> Dialeto {
        match self {
            Preset::Anthropic => Dialeto::Anthropic,
            _ => Dialeto::OpenAi,
        }
    }

    /// A URL base que o preset preenche (vazia no personalizado). Não há nome de modelo aqui:
    /// os nomes envelhecem em meses, e a lista vem do próprio provedor.
    pub fn url(self) -> &'static str {
        match self {
            Preset::Ollama => "http://localhost:11434/v1",
            Preset::LmStudio => "http://localhost:1234/v1",
            Preset::OpenRouter => "https://openrouter.ai/api/v1",
            Preset::OpenAi => "https://api.openai.com/v1",
            Preset::Anthropic => "https://api.anthropic.com/v1",
            Preset::Groq => "https://api.groq.com/openai/v1",
            Preset::Personalizado => "",
        }
    }

    /// Se o provedor pede chave (os locais não).
    pub fn pede_chave(self) -> bool {
        !matches!(
            self,
            Preset::Ollama | Preset::LmStudio | Preset::Personalizado
        )
    }

    /// O nome curto, sem acento, para ids de widgets.
    pub fn chave(self) -> &'static str {
        match self {
            Preset::Ollama => "ollama",
            Preset::LmStudio => "lmstudio",
            Preset::OpenRouter => "openrouter",
            Preset::OpenAi => "openai",
            Preset::Anthropic => "anthropic",
            Preset::Groq => "groq",
            Preset::Personalizado => "personalizado",
        }
    }

    /// O preset que o texto de [`Preset::nome`] escolhe.
    pub fn do_rotulo(texto: &str) -> Option<Preset> {
        Preset::TODOS.into_iter().find(|p| p.nome() == texto)
    }

    /// O nome do preset.
    pub fn nome(self) -> String {
        match self {
            Preset::Ollama => t!("ia-preset-ollama"),
            Preset::LmStudio => t!("ia-preset-lmstudio"),
            Preset::OpenRouter => t!("ia-preset-openrouter"),
            Preset::OpenAi => t!("ia-preset-openai"),
            Preset::Anthropic => t!("ia-preset-anthropic"),
            Preset::Groq => t!("ia-preset-groq"),
            Preset::Personalizado => t!("ia-preset-personalizado"),
        }
    }

    /// A observação que acompanha o preset no formulário; vazia quando não há.
    pub fn observacao(self) -> String {
        match self {
            Preset::Ollama | Preset::LmStudio => t!("ia-preset-obs-local"),
            Preset::OpenRouter => t!("ia-preset-obs-openrouter"),
            Preset::Personalizado => t!("ia-preset-obs-personalizado"),
            _ => String::new(),
        }
    }

    /// O texto do cartão do "Escolher um provedor de IA".
    pub fn descricao_do_cartao(self) -> String {
        match self {
            Preset::Ollama => t!("ia-cartao-ollama"),
            Preset::LmStudio => t!("ia-cartao-lmstudio"),
            Preset::OpenRouter => t!("ia-cartao-openrouter"),
            Preset::OpenAi => t!("ia-cartao-openai"),
            Preset::Anthropic => t!("ia-cartao-anthropic"),
            Preset::Groq => t!("ia-cartao-groq"),
            Preset::Personalizado => t!("ia-cartao-personalizado"),
        }
    }

    /// A dica que o botão "Testar" dá quando o provedor local não responde.
    pub fn dica_de_conexao(self) -> Option<String> {
        match self {
            Preset::Ollama => Some(t!("ia-dica-ollama")),
            Preset::LmStudio => Some(t!("ia-dica-lmstudio")),
            _ => None,
        }
    }
}

/// Um provedor de IA configurado.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerfilIa {
    /// O id (único no arquivo).
    pub id: u64,
    /// O nome que aparece nas listas.
    pub nome: String,
    /// De que preset veio. Vira [`Preset::Personalizado`] se a URL mudar.
    pub preset: Preset,
    /// O formato do pedido.
    pub dialeto: Dialeto,
    /// A base, até o `/v1`.
    pub url: String,
    /// O nome do modelo (vazio: o provedor escolhe, se aceitar).
    pub modelo: String,
    /// O item do cofre (tipo Senha) com a chave. `None`: sem chave.
    pub credencial: Option<u64>,
    /// Por onde o pedido sai: direto ou por um perfil de rede (M34). `Herdar` não faz sentido
    /// aqui e vale como direta.
    pub rede: Rede,
    /// Cabeçalhos a mais, na ordem.
    pub cabecalhos: Vec<(String, String)>,
    /// O teto de tokens da resposta (`None`: o do provedor).
    pub max_tokens: Option<u32>,
    /// A temperatura (`None`: a do provedor).
    pub temperatura: Option<f32>,
    /// Como o certificado é conferido.
    pub tls: TlsIa,
    /// A pessoa já viu e aceitou o aviso de endereço fora da máquina e da rede local.
    pub aviso_saida_aceito: bool,
}

impl Default for PerfilIa {
    fn default() -> Self {
        Self {
            id: 0,
            nome: String::new(),
            preset: Preset::Personalizado,
            dialeto: Dialeto::OpenAi,
            url: String::new(),
            modelo: String::new(),
            credencial: None,
            rede: Rede::Direta,
            cabecalhos: Vec::new(),
            max_tokens: None,
            temperatura: None,
            tls: TlsIa::Sistema,
            aviso_saida_aceito: false,
        }
    }
}

impl PerfilIa {
    /// Um perfil novo, preenchido pelo preset (o nome é o do preset).
    pub fn do_preset(id: u64, preset: Preset) -> Self {
        Self {
            id,
            nome: preset.nome(),
            preset,
            dialeto: preset.dialeto(),
            url: preset.url().to_owned(),
            ..Self::default()
        }
    }

    /// Troca o preset e preenche o dialeto e a URL dele (o personalizado não mexe em nada).
    pub fn trocar_preset(&mut self, preset: Preset) {
        self.preset = preset;
        if preset != Preset::Personalizado {
            self.dialeto = preset.dialeto();
            self.url = preset.url().to_owned();
        }
    }

    /// A pessoa mudou a URL: se não é mais a do preset, ele vira personalizado.
    pub fn url_editada(&mut self, url: String) {
        self.url = url;
        if self.preset != Preset::Personalizado && self.url.trim() != self.preset.url() {
            self.preset = Preset::Personalizado;
        }
    }

    /// A URL sem espaços nem barra no fim.
    pub fn url_limpa(&self) -> String {
        self.url.trim().trim_end_matches('/').to_owned()
    }

    /// Onde fica o endereço: neste computador, na rede local ou na internet.
    pub fn localidade(&self) -> Locality {
        locality(&self.url_limpa())
    }

    /// A rede do pedido: a do perfil, ou direta no lugar de `Herdar`.
    pub fn rede_efetiva(&self) -> Rede {
        match self.rede {
            Rede::Herdar => Rede::Direta,
            outra => outra,
        }
    }

    /// Se o perfil tem o que precisa para um pedido: URL com esquema e o dialeto.
    pub fn usavel(&self) -> bool {
        let url = self.url_limpa();
        url.starts_with("http://") || url.starts_with("https://")
    }

    /// O que a lib recebe. A chave e o proxy são resolvidos por quem chama (o cofre e a rede
    /// ficam na interface), antes de abrir a thread do pedido.
    pub fn endpoint(&self, chave: Option<Zeroizing<String>>, proxy: Option<Proxy>) -> Endpoint {
        let mut ponto = Endpoint::new(self.dialeto.da_lib(), self.url_limpa(), self.modelo.trim());
        ponto.api_key = chave.filter(|c| !c.is_empty());
        ponto.headers = self
            .cabecalhos
            .iter()
            .filter(|(nome, _)| !nome.trim().is_empty())
            .map(|(nome, valor)| (nome.trim().to_owned(), valor.trim().to_owned()))
            .collect();
        ponto.proxy = proxy;
        ponto.tls = self.tls.da_lib();
        ponto
    }
}

/// O conteúdo do `ia.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArquivoIa {
    /// A versão do formato.
    pub versao: u32,
    /// Os provedores.
    pub perfis: Vec<PerfilIa>,
    /// O provedor padrão (`None`: o primeiro).
    pub padrao: Option<u64>,
}

impl Default for ArquivoIa {
    fn default() -> Self {
        Self {
            versao: VERSAO,
            perfis: Vec::new(),
            padrao: None,
        }
    }
}

impl ArquivoIa {
    /// Lê o `ia.json`. Ausente dá a lista vazia; corrompido vai para `ia.json.bak` e o aviso diz
    /// onde ficou a cópia.
    pub fn carregar(armazenamento: &Armazenamento) -> (Self, Option<String>) {
        armazenamento.carregar_json(&armazenamento.arquivo_ia(), &t!("ia-o-que"))
    }

    /// Grava o `ia.json` (atômico).
    pub fn salvar(&self, armazenamento: &Armazenamento) -> std::io::Result<()> {
        armazenamento.gravar(&armazenamento.arquivo_ia(), self)
    }

    /// O próximo id livre.
    pub fn proximo_id(&self) -> u64 {
        self.perfis.iter().map(|p| p.id).max().unwrap_or(0) + 1
    }

    /// O perfil de id dado.
    pub fn perfil(&self, id: u64) -> Option<&PerfilIa> {
        self.perfis.iter().find(|p| p.id == id)
    }

    /// O perfil de id dado, para mudar.
    pub fn perfil_mut(&mut self, id: u64) -> Option<&mut PerfilIa> {
        self.perfis.iter_mut().find(|p| p.id == id)
    }

    /// O perfil padrão: o escolhido, se ainda existe, ou o primeiro.
    pub fn padrao_efetivo(&self) -> Option<&PerfilIa> {
        self.padrao
            .and_then(|id| self.perfil(id))
            .or_else(|| self.perfis.first())
    }

    /// O perfil de uma conexão: o dela, se existe; senão o padrão. Um id que não existe mais (a
    /// importação de outra máquina, um perfil apagado) cai no padrão.
    pub fn perfil_da_conexao(&self, escolhido: Option<u64>) -> Option<&PerfilIa> {
        escolhido
            .and_then(|id| self.perfil(id))
            .or_else(|| self.padrao_efetivo())
    }

    /// Acrescenta um perfil já preenchido (com o id novo) e devolve o id. O primeiro vira o
    /// padrão.
    pub fn adicionar(&mut self, mut perfil: PerfilIa) -> u64 {
        perfil.id = self.proximo_id();
        let id = perfil.id;
        self.perfis.push(perfil);
        if self.padrao.is_none() {
            self.padrao = Some(id);
        }
        id
    }

    /// Apaga o perfil e devolve o item do cofre que ele usava (a tela pergunta se apaga também).
    /// Se era o padrão, o padrão passa para o primeiro que sobrar.
    pub fn remover(&mut self, id: u64) -> Option<Option<u64>> {
        let posicao = self.perfis.iter().position(|p| p.id == id)?;
        let perfil = self.perfis.remove(posicao);
        if self.padrao == Some(id) {
            self.padrao = self.perfis.first().map(|p| p.id);
        }
        Some(perfil.credencial)
    }

    /// Dá ao perfil um nome que ninguém usa ("OpenAI", "OpenAI 2"…).
    pub fn nome_livre(&self, base: &str) -> String {
        let usado = |nome: &str| self.perfis.iter().any(|p| p.nome == nome);
        if !usado(base) {
            return base.to_owned();
        }
        (2u32..)
            .map(|n| format!("{base} {n}"))
            .find(|nome| !usado(nome))
            .unwrap_or_else(|| base.to_owned())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn presets_preenchem_dialeto_e_url_sem_modelo() {
        for preset in Preset::TODOS {
            let perfil = PerfilIa::do_preset(1, preset);
            assert_eq!(perfil.dialeto, preset.dialeto());
            assert_eq!(perfil.url, preset.url());
            assert_eq!(perfil.modelo, "", "nenhum nome de modelo fixo no código");
            assert_eq!(perfil.preset, preset);
        }
        assert_eq!(Preset::Ollama.url(), "http://localhost:11434/v1");
        assert_eq!(Preset::LmStudio.url(), "http://localhost:1234/v1");
        assert_eq!(Preset::Anthropic.dialeto(), Dialeto::Anthropic);
        assert_eq!(Preset::Groq.dialeto(), Dialeto::OpenAi);
        assert_eq!(Preset::Personalizado.url(), "");
    }

    #[test]
    fn so_os_provedores_da_internet_pedem_chave() {
        let pedem: Vec<Preset> = Preset::TODOS
            .into_iter()
            .filter(|p| p.pede_chave())
            .collect();
        assert_eq!(
            pedem,
            [
                Preset::OpenRouter,
                Preset::OpenAi,
                Preset::Anthropic,
                Preset::Groq
            ]
        );
    }

    #[test]
    fn mudar_a_url_faz_o_preset_virar_personalizado() {
        let mut perfil = PerfilIa::do_preset(1, Preset::OpenAi);
        perfil.url_editada("https://api.openai.com/v1".into());
        assert_eq!(perfil.preset, Preset::OpenAi);
        perfil.url_editada("https://meu-proxy.exemplo/v1".into());
        assert_eq!(perfil.preset, Preset::Personalizado);
        // O dialeto fica como estava.
        assert_eq!(perfil.dialeto, Dialeto::OpenAi);
    }

    #[test]
    fn trocar_o_preset_preenche_e_o_personalizado_nao_mexe() {
        let mut perfil = PerfilIa::do_preset(1, Preset::Ollama);
        perfil.trocar_preset(Preset::Anthropic);
        assert_eq!(perfil.dialeto, Dialeto::Anthropic);
        assert_eq!(perfil.url, "https://api.anthropic.com/v1");
        perfil.trocar_preset(Preset::Personalizado);
        assert_eq!(perfil.url, "https://api.anthropic.com/v1");
        assert_eq!(perfil.preset, Preset::Personalizado);
    }

    #[test]
    fn localidade_pela_url() {
        let mut perfil = PerfilIa::do_preset(1, Preset::Ollama);
        assert_eq!(perfil.localidade(), Locality::Machine);
        perfil.url_editada("http://192.168.0.7:11434/v1/".into());
        assert_eq!(perfil.localidade(), Locality::Lan);
        assert_eq!(perfil.url_limpa(), "http://192.168.0.7:11434/v1");
        perfil.url_editada("https://api.openai.com/v1".into());
        assert_eq!(perfil.localidade(), Locality::Internet);
    }

    #[test]
    fn endpoint_leva_url_modelo_chave_cabecalhos_e_tls() {
        let mut perfil = PerfilIa::do_preset(1, Preset::OpenRouter);
        perfil.modelo = " um/modelo ".into();
        perfil.cabecalhos = vec![
            ("X-Title".into(), " Tirech ".into()),
            ("  ".into(), "vazio".into()),
        ];
        perfil.tls = TlsIa::CaPropria("/etc/ca.pem".into());
        let ponto = perfil.endpoint(Some(Zeroizing::new("sk-teste".into())), None);
        assert_eq!(ponto.dialect, Dialect::OpenAi);
        assert_eq!(ponto.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(ponto.model, "um/modelo");
        assert_eq!(
            ponto.api_key.as_deref().map(String::as_str),
            Some("sk-teste")
        );
        assert_eq!(ponto.headers, vec![("X-Title".into(), "Tirech".into())]);
        assert_eq!(ponto.tls, Tls::CustomCa(PathBuf::from("/etc/ca.pem")));
        assert!(ponto.proxy.is_none());
        // O `Debug` do endpoint não mostra a chave.
        assert!(!format!("{ponto:?}").contains("sk-teste"));
    }

    #[test]
    fn chave_vazia_vale_como_sem_chave() {
        let perfil = PerfilIa::do_preset(1, Preset::Ollama);
        let ponto = perfil.endpoint(Some(Zeroizing::new(String::new())), None);
        assert!(ponto.api_key.is_none());
    }

    #[test]
    fn rede_herdar_vale_como_direta() {
        let mut perfil = PerfilIa {
            rede: Rede::Herdar,
            ..PerfilIa::default()
        };
        assert_eq!(perfil.rede_efetiva(), Rede::Direta);
        perfil.rede = Rede::Perfil(3);
        assert_eq!(perfil.rede_efetiva(), Rede::Perfil(3));
    }

    #[test]
    fn usavel_pede_http_ou_https() {
        let mut perfil = PerfilIa::default();
        assert!(!perfil.usavel());
        perfil.url = "localhost:11434/v1".into();
        assert!(!perfil.usavel());
        perfil.url = " http://localhost:11434/v1 ".into();
        assert!(perfil.usavel());
    }

    #[test]
    fn o_primeiro_perfil_vira_o_padrao() {
        let mut arquivo = ArquivoIa::default();
        let a = arquivo.adicionar(PerfilIa::do_preset(0, Preset::Ollama));
        let b = arquivo.adicionar(PerfilIa::do_preset(0, Preset::OpenAi));
        assert_eq!((a, b), (1, 2));
        assert_eq!(arquivo.padrao, Some(1));
        assert_eq!(arquivo.padrao_efetivo().map(|p| p.id), Some(1));
    }

    #[test]
    fn remover_passa_o_padrao_adiante_e_devolve_o_item_do_cofre() {
        let mut arquivo = ArquivoIa::default();
        let a = arquivo.adicionar(PerfilIa::do_preset(0, Preset::Ollama));
        let mut com_chave = PerfilIa::do_preset(0, Preset::OpenAi);
        com_chave.credencial = Some(77);
        let b = arquivo.adicionar(com_chave);
        assert_eq!(arquivo.remover(a), Some(None));
        assert_eq!(arquivo.padrao, Some(b));
        assert_eq!(arquivo.remover(b), Some(Some(77)));
        assert_eq!(arquivo.padrao, None);
        assert_eq!(arquivo.remover(b), None);
        assert!(arquivo.padrao_efetivo().is_none());
    }

    #[test]
    fn padrao_apagado_cai_no_primeiro() {
        let mut arquivo = ArquivoIa::default();
        arquivo.adicionar(PerfilIa::do_preset(0, Preset::Ollama));
        arquivo.adicionar(PerfilIa::do_preset(0, Preset::OpenAi));
        arquivo.padrao = Some(99);
        assert_eq!(arquivo.padrao_efetivo().map(|p| p.id), Some(1));
    }

    #[test]
    fn perfil_da_conexao_cai_no_padrao_quando_o_id_nao_existe() {
        let mut arquivo = ArquivoIa::default();
        arquivo.adicionar(PerfilIa::do_preset(0, Preset::Ollama));
        let segundo = arquivo.adicionar(PerfilIa::do_preset(0, Preset::OpenAi));
        assert_eq!(
            arquivo.perfil_da_conexao(Some(segundo)).map(|p| p.id),
            Some(2)
        );
        assert_eq!(arquivo.perfil_da_conexao(Some(42)).map(|p| p.id), Some(1));
        assert_eq!(arquivo.perfil_da_conexao(None).map(|p| p.id), Some(1));
        assert!(ArquivoIa::default().perfil_da_conexao(Some(1)).is_none());
    }

    #[test]
    fn nome_livre_acrescenta_numero() {
        let mut arquivo = ArquivoIa::default();
        arquivo.adicionar(PerfilIa {
            nome: "OpenAI".into(),
            ..PerfilIa::default()
        });
        arquivo.adicionar(PerfilIa {
            nome: "OpenAI 2".into(),
            ..PerfilIa::default()
        });
        assert_eq!(arquivo.nome_livre("Groq"), "Groq");
        assert_eq!(arquivo.nome_livre("OpenAI"), "OpenAI 3");
    }

    #[test]
    fn json_vai_e_volta_sem_a_chave() {
        let mut arquivo = ArquivoIa::default();
        let mut perfil = PerfilIa::do_preset(0, Preset::Anthropic);
        perfil.credencial = Some(12);
        perfil.modelo = "algum-modelo".into();
        perfil.cabecalhos = vec![("X-A".into(), "b".into())];
        perfil.tls = TlsIa::CaPropria("/x.pem".into());
        perfil.max_tokens = Some(2048);
        perfil.temperatura = Some(0.2);
        perfil.rede = Rede::Perfil(5);
        arquivo.adicionar(perfil);
        let texto = serde_json::to_string_pretty(&arquivo).unwrap();
        // O perfil só sabe o id do item do cofre; não há campo de chave.
        assert!(texto.contains("\"credencial\": 12"));
        assert!(!texto.to_lowercase().contains("api_key"));
        assert!(!texto.to_lowercase().contains("chave"));
        let volta: ArquivoIa = serde_json::from_str(&texto).unwrap();
        assert_eq!(volta, arquivo);
    }

    #[test]
    fn carregar_e_salvar_pelo_armazenamento() {
        use crate::armazenamento::testes::PastaTemporaria;
        let pasta = PastaTemporaria::nova("ia-ida-e-volta");
        let armazenamento = Armazenamento::new(&pasta.0);
        let (vazio, aviso) = ArquivoIa::carregar(&armazenamento);
        assert!(aviso.is_none());
        assert!(vazio.perfis.is_empty());
        let mut arquivo = ArquivoIa::default();
        arquivo.adicionar(PerfilIa::do_preset(0, Preset::Ollama));
        arquivo.salvar(&armazenamento).unwrap();
        assert!(pasta.0.join("ia.json").is_file());
        let (lido, aviso) = ArquivoIa::carregar(&armazenamento);
        assert!(aviso.is_none());
        assert_eq!(lido, arquivo);
    }

    #[test]
    fn arquivo_corrompido_vai_para_bak_com_aviso() {
        use crate::armazenamento::testes::PastaTemporaria;
        let pasta = PastaTemporaria::nova("ia-corrompido");
        let armazenamento = Armazenamento::new(&pasta.0);
        std::fs::create_dir_all(&pasta.0).unwrap();
        std::fs::write(pasta.0.join("ia.json"), "{ isto não é json").unwrap();
        let (lido, aviso) = ArquivoIa::carregar(&armazenamento);
        assert!(lido.perfis.is_empty());
        assert!(aviso.is_some_and(|a| a.contains("ia.json.bak")));
        assert!(pasta.0.join("ia.json.bak").is_file());
        assert!(!pasta.0.join("ia.json").exists());
    }

    #[test]
    fn json_com_campos_faltando_usa_os_padroes() {
        let lido: ArquivoIa = serde_json::from_str(
            r#"{"perfis":[{"id":3,"nome":"X","url":"http://localhost:1234/v1"}]}"#,
        )
        .unwrap();
        assert_eq!(lido.versao, VERSAO);
        let perfil = &lido.perfis[0];
        assert_eq!(perfil.dialeto, Dialeto::OpenAi);
        assert_eq!(perfil.tls, TlsIa::Sistema);
        assert_eq!(perfil.rede, Rede::Direta);
        assert!(!perfil.aviso_saida_aceito);
        assert_eq!(lido.padrao, None);
    }
}
