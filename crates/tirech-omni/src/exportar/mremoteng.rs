//! O `confCons.xml` do mRemoteNG: pastas (`Container`) e conexões SSH2 e Telnet. Com "incluir
//! senhas" elas saem cifradas com a senha-mestra padrão do mRemoteNG (`mR3m`), em AES-256-GCM,
//! que ele abre sem perguntar. O formato não tem campo de chave: o mRemoteNG usa a da sessão do
//! PuTTY de mesmo nome (`PuttySession`), por isso o resumo sugere exportar o `.reg` também.

use super::plano::{NoPlano, PastaPlano};
use super::putty::nome_da_sessao;
use super::{Aleatorio, ItemPlano, NomesUnicos, Opcoes, Plano, Resultado, escapar_xml};
use crate::importar::cripto::{
    ITERACOES_PADRAO, PROTEGIDO_NAO, SENHA_MESTRA_PADRAO, mremoteng_cifrar,
};
use crate::modelo::Kind;
use crate::t;

/// O que se carrega de nó em nó.
struct Escrita<'a> {
    opcoes: Opcoes,
    aleatorio: &'a mut dyn Aleatorio,
    /// O nome da sessão do PuTTY de cada conexão, na ordem de `itens`.
    sessoes: Vec<(*const ItemPlano, String)>,
    saida: String,
    falhas: usize,
}

fn guid(aleatorio: &mut dyn Aleatorio) -> String {
    let mut b = [0u8; 16];
    aleatorio.preencher(&mut b);
    // Versão 4 (aleatório).
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    )
}

fn cifrar(texto: &str, aleatorio: &mut dyn Aleatorio) -> Result<String, String> {
    let (mut sal, mut nonce) = ([0u8; 16], [0u8; 16]);
    aleatorio.preencher(&mut sal);
    aleatorio.preencher(&mut nonce);
    mremoteng_cifrar(texto, SENHA_MESTRA_PADRAO, ITERACOES_PADRAO, sal, nonce)
}

impl Escrita<'_> {
    fn sessao_de(&self, item: &ItemPlano) -> String {
        self.sessoes
            .iter()
            .find(|(p, _)| std::ptr::eq(*p, item))
            .map(|(_, nome)| nome.clone())
            .unwrap_or_else(|| "Default Settings".to_owned())
    }

    fn conexao(&mut self, item: &ItemPlano, recuo: &str) {
        let c = &item.conexao;
        let senha = match item.senha.as_deref().filter(|_| self.opcoes.senhas) {
            Some(senha) => cifrar(senha, self.aleatorio).unwrap_or_else(|_| {
                self.falhas += 1;
                String::new()
            }),
            None => String::new(),
        };
        let id = guid(self.aleatorio);
        self.saida.push_str(&format!(
            "{recuo}<Node Name=\"{}\" Type=\"Connection\" Descr=\"{}\" Icon=\"mRemoteNG\" Panel=\"General\" \
             Id=\"{id}\" Username=\"{}\" Domain=\"\" Password=\"{senha}\" Hostname=\"{}\" \
             Protocol=\"{}\" PuttySession=\"{}\" Port=\"{}\" InheritUsername=\"false\" \
             InheritPassword=\"false\" />\n",
            escapar_xml(&c.name),
            escapar_xml(&c.descricao),
            escapar_xml(&c.user),
            escapar_xml(&c.host),
            if c.kind == Kind::Telnet { "Telnet" } else { "SSH2" },
            escapar_xml(&self.sessao_de(item)),
            c.port,
        ));
    }

    fn pasta(&mut self, p: &PastaPlano, recuo: &str) {
        let id = guid(self.aleatorio);
        self.saida.push_str(&format!(
            "{recuo}<Node Name=\"{}\" Type=\"Container\" Expanded=\"false\" Descr=\"\" Icon=\"Folder\" \
             Panel=\"General\" Id=\"{id}\" Username=\"\" Domain=\"\" Password=\"\" \
             InheritUsername=\"false\" InheritPassword=\"false\">\n",
            escapar_xml(&p.nome)
        ));
        self.nos(&p.filhos, &format!("{recuo}  "));
        self.saida.push_str(&format!("{recuo}</Node>\n"));
    }

    fn nos(&mut self, lista: &[NoPlano], recuo: &str) {
        for no in lista {
            match no {
                NoPlano::Pasta(p) => self.pasta(p, recuo),
                NoPlano::Item(item) => self.conexao(item, recuo),
            }
        }
    }
}

/// Escreve o arquivo.
pub fn exportar(
    plano: &Plano,
    itens: &[&ItemPlano],
    opcoes: Opcoes,
    aleatorio: &mut dyn Aleatorio,
) -> Resultado {
    let mut resultado = Resultado::default();
    // As mesmas sessões que o `.reg` do PuTTY terá (as duas exportações aceitam SSH e Telnet).
    let mut nomes = NomesUnicos::default();
    let sessoes = itens
        .iter()
        .map(|i| {
            (
                std::ptr::from_ref::<ItemPlano>(i),
                nomes.unico(&nome_da_sessao(i)),
            )
        })
        .collect();
    let protegido = cifrar(PROTEGIDO_NAO, aleatorio).unwrap_or_default();
    let mut escrita = Escrita {
        opcoes,
        aleatorio,
        sessoes,
        saida: format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<Connections Name=\"Connections\" Export=\"false\" \
             EncryptionEngine=\"AES\" BlockCipherMode=\"GCM\" KdfIterations=\"{ITERACOES_PADRAO}\" \
             FullFileEncryption=\"false\" Protected=\"{protegido}\" ConfVersion=\"2.6\">\n"
        ),
        falhas: 0,
    };
    escrita.nos(&plano.aninhar(itens), "  ");
    escrita.saida.push_str("</Connections>\n");
    let falhas = escrita.falhas;
    resultado.texto = escrita.saida;
    if falhas > 0 {
        resultado.avisar(t!("exportar-mremoteng-falhas", n = falhas as i64));
    }
    let com_chave = itens
        .iter()
        .filter(|i| i.conexao.key_file.is_some() || i.usa_chave_do_cofre)
        .count();
    if com_chave > 0 {
        resultado.avisar(t!("exportar-mremoteng-chave"));
    }
    let com_rede = itens
        .iter()
        .filter(|i| i.proxy.is_some() || !i.saltos.is_empty())
        .count();
    if com_rede > 0 {
        resultado.avisar(t!("exportar-mremoteng-rede", n = com_rede as i64));
    }
    resultado
}
