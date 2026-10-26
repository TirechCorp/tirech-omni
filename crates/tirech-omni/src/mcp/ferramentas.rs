//! A lista das ferramentas que o app oferece aos assistentes (`tools/list`), com o esquema de
//! cada uma. A ponte responde a lista sozinha, sem o app aberto, e o app confere os argumentos
//! pelos mesmos nomes. Os textos são os que o assistente lê: em português, curtos, e cada um
//! diz a permissão que a ferramenta exige.

use serde_json::{Map, Value, json};

/// As ferramentas do espaço de trabalho remoto: com `--espaco`, não pedem o argumento `espaco`.
pub const DO_ESPACO: [&str; 6] = ["ler", "editar", "escrever", "buscar", "glob", "bash"];

/// As duas ferramentas de painéis: a permissão delas depende de todas as conexões liberadas,
/// não de uma só, e por isso não estão em [`super::permissao::ferramentas_conhecidas`].
pub const DE_PAINEIS: [&str; 2] = ["listar_paineis", "organizar_paineis"];

/// A lista do `tools/list`. `espaco_fixo` é o nome que a ponte recebeu em `--espaco`: as
/// ferramentas do espaço de trabalho não pedem o argumento `espaco` e os textos dizem isso.
/// `aba_fixa` diz que a ponte recebeu `--aba` (M63): o argumento `aba` das ferramentas de
/// terminal passa a ser opcional e a descrição conta qual aba vale sem ele.
pub fn lista(espaco_fixo: Option<&str>, aba_fixa: bool) -> Vec<Value> {
    let mut todas = operacao();
    todas.extend(espaco(espaco_fixo));
    if aba_fixa {
        for ferramenta in &mut todas {
            // Com `get_mut`: o índice com `[]` criaria um `aba` sem tipo nas ferramentas que não
            // o têm, e o Claude Code recusa a ferramenta inteira por isso.
            let descricao = ferramenta
                .pointer_mut("/inputSchema/properties/aba/description")
                .filter(|d| d.as_str() == Some(ABA));
            if let Some(descricao) = descricao {
                *descricao = Value::from(ABA_FIXA);
            }
        }
    }
    todas
}

/// O nome de cada ferramenta da lista, na ordem dela.
pub fn nomes(espaco_fixo: Option<&str>) -> Vec<String> {
    lista(espaco_fixo, false)
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_owned))
        .collect()
}

fn texto(descricao: &str) -> Value {
    json!({ "type": "string", "description": descricao })
}

fn inteiro(descricao: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": descricao })
}

fn booleano(descricao: &str) -> Value {
    json!({ "type": "boolean", "description": descricao })
}

/// Uma conexão pelo id (número) ou pelo caminho na árvore (texto).
fn conexao(descricao: &str) -> Value {
    json!({ "type": ["string", "integer"], "description": descricao })
}

/// Uma opção entre `valores`.
fn escolha(valores: &[&str], descricao: &str) -> Value {
    json!({ "type": "string", "enum": valores, "description": descricao })
}

/// O esquema de uma ferramenta: objeto fechado, com os argumentos `propriedades` e os
/// `obrigatorios` entre eles.
fn ferramenta(
    nome: &str,
    descricao: &str,
    propriedades: Vec<(&str, Value)>,
    obrigatorios: &[&str],
) -> Value {
    let mut props = Map::new();
    for (chave, valor) in propriedades {
        props.insert(chave.to_owned(), valor);
    }
    json!({
        "name": nome,
        "description": descricao,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": obrigatorios,
            "additionalProperties": false,
        },
    })
}

const ABA: &str = "A aba (a chave que listar_abas devolve). Sem ela, vale a aba ativa da janela em foco, se for de uma conexão liberada.";

/// O mesmo argumento numa ponte presa a uma aba (`--aba`, M63).
const ABA_FIXA: &str = "A aba (a chave que listar_abas devolve). Opcional: sem ela, vale a aba que abriu este assistente (a marcada com alvo em listar_abas).";
const CONEXAO: &str = "A conexão: o id (número) ou o caminho na árvore, como \"Clientes/ACME/api-prod\". O nome sozinho vale quando é único.";

fn operacao() -> Vec<Value> {
    vec![
        ferramenta(
            "listar_conexoes",
            "Lista as conexões que a pessoa liberou para assistentes, com id, caminho, tipo, a permissão e quantas abas têm; host, porta e usuário só nos tipos em que valem (o shell local e o S3 não têm). Exige: ver.",
            vec![(
                "filtro",
                texto(
                    "Parte do nome, host, usuário, caminho ou etiqueta; não diferencia maiúsculas nem acentos.",
                ),
            )],
            &[],
        ),
        ferramenta(
            "abrir_conexao",
            "Abre uma conexão numa aba, como se a pessoa clicasse nela, e espera conectar. Se faltar senha ou frase da chave, a pessoa responde no app: você não vê nem digita a senha. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                (
                    "janela",
                    texto("\"atual\" (padrão), \"nova\" ou o número da janela."),
                ),
                (
                    "painel",
                    texto("O número do painel, \"direita\" ou \"nova_janela\"."),
                ),
            ],
            &["conexao"],
        ),
        ferramenta(
            "listar_abas",
            "Lista as abas abertas das conexões liberadas: aba, conexão, título, estado, janela, painel, se é a ativa, se foi aberta por assistente e de quem. Exige: ver.",
            vec![("conexao", conexao(CONEXAO))],
            &[],
        ),
        ferramenta(
            "fechar_aba",
            "Fecha uma aba. Exige: mudar.",
            vec![("aba", texto("A aba a fechar (a chave de listar_abas)."))],
            &["aba"],
        ),
        ferramenta(
            "reconectar_aba",
            "Reconecta uma aba caída, a mesma aba, com o histórico da tela, e espera o servidor responder; devolve o estado e a tela. A aba que um assistente usa já reconecta sozinha algumas vezes (listar_abas mostra \"reconectando\"); use esta quando ela ficou \"caida\". Se faltar senha, a pessoa responde no app. Exige: mudar.",
            vec![("aba", texto("A aba a reconectar (a chave de listar_abas)."))],
            &["aba"],
        ),
        ferramenta(
            "digitar",
            "Digita texto na aba, como o teclado. Use \\n para o Enter. Exige: mudar.",
            vec![
                ("aba", texto(ABA)),
                ("texto", texto("O que digitar.")),
                (
                    "colar",
                    booleano(
                        "Usa o colar entre colchetes quando o programa pediu, como o Ctrl+Shift+V.",
                    ),
                ),
            ],
            &["texto"],
        ),
        ferramenta(
            "teclas",
            "Aperta teclas na aba, na ordem: \"Enter\", \"Tab\", \"Esc\", \"Ctrl+C\", \"Ctrl+D\", \"Up\", \"F2\", \"Alt+X\", \"PageDown\"… Exige: mudar.",
            vec![
                ("aba", texto(ABA)),
                (
                    "teclas",
                    json!({ "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Os nomes das teclas." }),
                ),
            ],
            &["teclas"],
        ),
        ferramenta(
            "ler_tela",
            "Lê o texto que está na tela da aba, com as linhas sem os espaços do fim. Exige: ver.",
            vec![
                ("aba", texto(ABA)),
                (
                    "historico",
                    inteiro("Quantas linhas do histórico incluir acima da tela (padrão 0)."),
                ),
                (
                    "cursor",
                    booleano("Acrescenta a linha com a posição do cursor."),
                ),
                (
                    "caracteres",
                    inteiro("Muda só nesta chamada o tamanho máximo da resposta."),
                ),
            ],
            &[],
        ),
        ferramenta(
            "esperar",
            "Espera algo aparecer na aba, ficar em silêncio ou o prazo acabar, e devolve o que chegou desde o pedido. Informe texto ou regex, não os dois. Exige: ver.",
            vec![
                ("aba", texto(ABA)),
                ("texto", texto("Um texto a esperar.")),
                ("regex", texto("Uma expressão regular a esperar.")),
                (
                    "silencio_secs",
                    inteiro("Termina quando a aba fica tantos segundos sem saída."),
                ),
                ("prazo_secs", inteiro("O prazo, em segundos.")),
                (
                    "caracteres",
                    inteiro("Muda só nesta chamada o tamanho máximo da resposta."),
                ),
            ],
            &["prazo_secs"],
        ),
        ferramenta(
            "executar",
            "Roda um comando. No modo \"aba\" (padrão) digita na aba, que precisa estar no prompt, e devolve a saída; o código de saída não volta (se precisar dele, termine o comando com ; echo \"código: $?\"). No modo \"separado\" (só SSH) roda à parte, sem aparecer na aba, e devolve a saída e o código; se a pessoa ligou \"rodar os comandos na aba\" (o padrão), o app o roda na aba mesmo assim quando ela está livre. O campo \"modo\" da resposta diz onde rodou. Exige: mudar.",
            vec![
                ("aba", texto(ABA)),
                ("comando", texto("O comando.")),
                (
                    "prazo_secs",
                    inteiro("O prazo, em segundos; sem ele, espera até terminar ou você cancelar."),
                ),
                (
                    "modo",
                    escolha(&["aba", "separado"], "Onde rodar (padrão: aba)."),
                ),
                (
                    "caracteres",
                    inteiro("Muda só nesta chamada o tamanho máximo da resposta."),
                ),
            ],
            &["comando"],
        ),
        ferramenta(
            "listar_arquivos",
            "Lista uma pasta do servidor: nome, tipo, tamanho, data e permissões. Exige: ver.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("aba", texto(ABA)),
                ("caminho", texto("A pasta, no servidor.")),
            ],
            &["caminho"],
        ),
        ferramenta(
            "baixar_arquivo",
            "Baixa um arquivo do servidor. Com destino, grava na pasta de troca do app; sem ele, devolve o conteúdo como texto (recusa binário). Exige: ver.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("aba", texto(ABA)),
                ("caminho", texto("O arquivo, no servidor.")),
                (
                    "destino",
                    texto("O caminho local, dentro da pasta de troca com o assistente."),
                ),
            ],
            &["caminho"],
        ),
        ferramenta(
            "enviar_arquivo",
            "Envia um arquivo local (da pasta de troca, ou um que a pessoa soltou ou colou no app) ou um texto para o servidor. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("aba", texto(ABA)),
                (
                    "origem",
                    texto(
                        "O arquivo local: dentro da pasta de troca com o assistente, ou um caminho que a pessoa soltou ou colou no app (ou dentro de uma pasta que ela soltou).",
                    ),
                ),
                ("conteudo", texto("O texto a gravar, no lugar de origem.")),
                ("caminho", texto("O destino, no servidor.")),
                (
                    "sobrescrever",
                    booleano("Troca o arquivo se ele já existe (padrão: não)."),
                ),
            ],
            &["caminho"],
        ),
        ferramenta(
            "listar_tuneis",
            "Lista os túneis das conexões: tipo, origem, destino e se estão ligados. Exige: ver.",
            vec![("conexao", conexao(CONEXAO))],
            &[],
        ),
        ferramenta(
            "ligar_tunel",
            "Liga um túnel da conexão, abrindo a aba dela se preciso. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("tunel", texto("O nome ou o índice do túnel na conexão.")),
            ],
            &["conexao", "tunel"],
        ),
        ferramenta(
            "desligar_tunel",
            "Desliga um túnel da conexão. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("tunel", texto("O nome ou o índice do túnel na conexão.")),
            ],
            &["conexao", "tunel"],
        ),
        ferramenta(
            "listar_portas",
            "Lista as portas TCP que escutam no servidor: alcance, serviço, processo, atalhos de apps conhecidos e o túnel daqui, se houver. Abre a aba da conexão se preciso. Exige: ver.",
            vec![("conexao", conexao(CONEXAO))],
            &["conexao"],
        ),
        ferramenta(
            "encaminhar_porta",
            "Encaminha uma porta do servidor para esta máquina (127.0.0.1) e devolve o endereço daqui, para usar com um cliente ou um curl. Reaproveita o túnel que já existe. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("porta", inteiro("A porta, vista do servidor.")),
                (
                    "host",
                    texto(
                        "O host, visto do servidor (padrão: 127.0.0.1). Use o IP de um contêiner para chegar a ele.",
                    ),
                ),
                (
                    "porta_local",
                    inteiro("A porta daqui (padrão: a mesma, ou a próxima livre)."),
                ),
                (
                    "host_http",
                    texto(
                        "Para páginas web: o nome de site que o servidor espera (o cabeçalho Host). O endereço devolvido usa {nome}.localhost.",
                    ),
                ),
                (
                    "https",
                    booleano(
                        "O servidor fala HTTPS (certificado de teste ou autoassinado é aceito). O endereço devolvido é local e sem TLS (padrão: não).",
                    ),
                ),
                (
                    "fixar",
                    booleano(
                        "Grava o túnel na conexão, para subir nas próximas sessões (padrão: não).",
                    ),
                ),
            ],
            &["conexao", "porta"],
        ),
        ferramenta(
            "parar_porta",
            "Para o túnel de uma porta do servidor, pela porta daqui que encaminhar_porta devolveu. Exige: mudar.",
            vec![
                ("conexao", conexao(CONEXAO)),
                ("porta_local", inteiro("A porta daqui do túnel.")),
            ],
            &["conexao", "porta_local"],
        ),
        ferramenta(
            "ler_saida",
            "Lê o resto de uma resposta que veio cortada, pelo id que ela deu. Exige: ver (a conexão da resposta original).",
            vec![
                ("saida", texto("O id da saída (s1, s2…).")),
                ("inicio", inteiro("O caractere onde começar.")),
                ("tamanho", inteiro("Quantos caracteres devolver.")),
            ],
            &["saida", "inicio"],
        ),
        ferramenta(
            "listar_paineis",
            "Mostra como as abas estão em painéis numa janela. Exige: ver, em alguma conexão liberada.",
            vec![("janela", texto("\"atual\" (padrão) ou o número da janela."))],
            &[],
        ),
        ferramenta(
            "organizar_paineis",
            "Muda a grade de painéis da janela. Exige: mudar, na permissão mais alta entre as conexões liberadas.",
            vec![
                ("janela", texto("\"atual\" (padrão) ou o número da janela.")),
                ("colunas", inteiro("Quantas colunas.")),
                ("linhas", inteiro("Quantas linhas.")),
            ],
            &["colunas", "linhas"],
        ),
    ]
}

/// As ferramentas do espaço de trabalho, mais `listar_espacos` quando a ponte não fixou um.
fn espaco(fixo: Option<&str>) -> Vec<Value> {
    // Com `--espaco` o argumento some; sem ele, é o primeiro e obrigatório.
    let com_espaco = |nome: &str,
                      descricao: &str,
                      mut props: Vec<(&'static str, Value)>,
                      mut obrigatorios: Vec<&'static str>|
     -> Value {
        if fixo.is_none() {
            props.insert(
                0,
                (
                    "espaco",
                    texto("O nome do espaço de trabalho (veja listar_espacos)."),
                ),
            );
            obrigatorios.insert(0, "espaco");
        }
        ferramenta(nome, descricao, props, &obrigatorios)
    };
    let onde = match fixo {
        Some(nome) => {
            format!(" Trabalha no espaço {nome}; caminhos relativos partem da raiz dele.")
        }
        None => " Os caminhos relativos partem da raiz do espaço.".to_owned(),
    };
    let mut todas = Vec::new();
    if fixo.is_none() {
        todas.push(ferramenta(
            "listar_espacos",
            "Lista os espaços de trabalho que a pessoa criou: nome, conexão, host e raiz. Exige: ver.",
            vec![],
            &[],
        ));
    }
    todas.push(com_espaco(
        "ler",
        &format!("Lê um arquivo do servidor, com as linhas numeradas. Exige: ver.{onde}"),
        vec![
            ("caminho", texto("O arquivo.")),
            ("inicio", inteiro("A primeira linha, a partir de 1.")),
            ("linhas", inteiro("Quantas linhas ler.")),
        ],
        vec!["caminho"],
    ));
    todas.push(com_espaco(
        "editar",
        &format!("Troca um trecho de um arquivo lido antes. O trecho antigo precisa casar exato. Exige: mudar.{onde}"),
        vec![
            ("caminho", texto("O arquivo.")),
            ("antigo", texto("O trecho a trocar, exato.")),
            ("novo", texto("O trecho novo.")),
            ("todas", booleano("Troca todas as ocorrências (padrão: não).")),
        ],
        vec!["caminho", "antigo", "novo"],
    ));
    todas.push(com_espaco(
        "escrever",
        &format!("Grava um arquivo inteiro, criando as pastas que faltam. Um arquivo que já existe precisa ter sido lido antes. Exige: mudar.{onde}"),
        vec![
            ("caminho", texto("O arquivo.")),
            ("conteudo", texto("O conteúdo inteiro.")),
        ],
        vec!["caminho", "conteudo"],
    ));
    todas.push(com_espaco(
        "buscar",
        &format!("Procura um texto nos arquivos. Regex é a sintaxe estendida do grep (ERE), sem lookaround nem referências. Exige: ver.{onde}"),
        vec![
            ("padrao", texto("O que procurar.")),
            ("modo", escolha(&["texto", "regex"], "Padrão: texto.")),
            ("caminho", texto("Uma subpasta ou arquivo (padrão: a raiz).")),
            ("glob", texto("Só os arquivos que casam com este padrão.")),
            ("maiusculas", booleano("Diferencia maiúsculas (padrão: sim).")),
            ("contexto", inteiro("Linhas antes e depois de cada achado.")),
            (
                "saida",
                escolha(&["linhas", "arquivos", "contagem"], "Padrão: linhas."),
            ),
            (
                "caracteres",
                inteiro("Muda só nesta chamada o tamanho máximo da resposta."),
            ),
        ],
        vec!["padrao"],
    ));
    todas.push(com_espaco(
        "glob",
        &format!("Acha arquivos pelo nome (*, **, ?, [abc], {{a,b}}); os mais recentes primeiro. Os que começam com ponto casam com *. Exige: ver.{onde}"),
        vec![
            ("padrao", texto("O padrão.")),
            ("caminho", texto("A pasta onde procurar (padrão: a raiz).")),
        ],
        vec!["padrao"],
    ));
    todas.push(com_espaco(
        "bash",
        &format!("Roda um comando no servidor, na raiz do espaço, sem terminal: cada chamada é um processo novo e o cd não vale para a próxima. Programas interativos não funcionam; para o que deve ficar rodando use nohup … &. Exige: mudar.{onde}"),
        vec![
            ("comando", texto("O comando.")),
            (
                "prazo_secs",
                inteiro("O prazo, em segundos; sem ele, roda até terminar ou você cancelar."),
            ),
            ("entrada", texto("Texto para a entrada padrão.")),
            (
                "caracteres",
                inteiro("Muda só nesta chamada o tamanho máximo da resposta."),
            ),
        ],
        vec!["comando"],
    ));
    todas
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::*;
    use crate::mcp::permissao::ferramentas_conhecidas;

    fn pede_espaco(f: &Value) -> bool {
        f["inputSchema"]["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|n| n == "espaco"))
    }

    #[test]
    fn lista_sem_espaco_pede_espaco() {
        let todas = lista(None, false);
        for nome in DO_ESPACO {
            let f = todas.iter().find(|f| f["name"] == nome).unwrap();
            assert!(pede_espaco(f), "{nome} devia pedir espaco");
            assert!(f["inputSchema"]["properties"]["espaco"].is_object());
        }
        assert!(todas.iter().any(|f| f["name"] == "listar_espacos"));
    }

    #[test]
    fn lista_com_espaco_nao_pede() {
        let todas = lista(Some("api"), false);
        for nome in DO_ESPACO {
            let f = todas.iter().find(|f| f["name"] == nome).unwrap();
            assert!(!pede_espaco(f), "{nome} não devia pedir espaco");
            assert!(f["inputSchema"]["properties"].get("espaco").is_none());
            assert!(f["description"].as_str().unwrap().contains("espaço api"));
        }
        assert!(!todas.iter().any(|f| f["name"] == "listar_espacos"));
        // As de operação continuam lá.
        assert!(todas.iter().any(|f| f["name"] == "listar_conexoes"));
    }

    #[test]
    fn todo_esquema_e_objeto_sem_extras() {
        for (espaco, aba_fixa) in [(None, false), (Some("api"), false), (None, true)] {
            for f in lista(espaco, aba_fixa) {
                let nome = f["name"].as_str().unwrap();
                let esquema = &f["inputSchema"];
                assert_eq!(esquema["type"], "object", "{nome}");
                assert_eq!(esquema["additionalProperties"], false, "{nome}");
                assert!(!f["description"].as_str().unwrap().is_empty(), "{nome}");
                let props = esquema["properties"].as_object().unwrap();
                // Cada argumento com tipo e descrição em texto: o Claude Code recusa a
                // ferramenta inteira se um faltar (foi o que tirou o listar_abas com `--aba`).
                for (chave, prop) in props {
                    assert!(!prop["type"].is_null(), "{nome}.{chave} sem tipo");
                    assert!(prop["description"].is_string(), "{nome}.{chave}");
                }
                for obrigatorio in esquema["required"].as_array().unwrap() {
                    let obrigatorio = obrigatorio.as_str().unwrap();
                    assert!(props.contains_key(obrigatorio), "{nome}.{obrigatorio}");
                }
            }
        }
    }

    #[test]
    fn nomes_unicos() {
        for espaco in [None, Some("api")] {
            let nomes = nomes(espaco);
            let unicos: HashSet<_> = nomes.iter().collect();
            assert_eq!(unicos.len(), nomes.len());
        }
    }

    #[test]
    fn exigencia_de_cada_ferramenta() {
        // Toda ferramenta da lista tem exigência definida, menos as de painéis, que dependem
        // de todas as conexões; e toda ferramenta com exigência está na lista.
        let conhecidas: HashSet<&str> = ferramentas_conhecidas().collect();
        let na_lista = nomes(None);
        for nome in &na_lista {
            if !DE_PAINEIS.contains(&nome.as_str()) {
                assert!(conhecidas.contains(nome.as_str()), "{nome} sem exigência");
            }
        }
        for nome in conhecidas {
            assert!(na_lista.iter().any(|n| n == nome), "{nome} fora da lista");
        }
        assert_eq!(na_lista.len(), 29);
    }
}
