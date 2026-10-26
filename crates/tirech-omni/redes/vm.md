# Quando a VPN só roda no Windows: uma VM

Alguns clientes só entregam um cliente de VPN proprietário, para Windows. Nesse caso a VPN
roda numa máquina virtual, e o app usa um proxy **dentro** dela, como usaria o de um
contêiner. Uma VM por cliente: cada uma tem a sua rota, e o mesmo IP em duas VMs não colide.

## A VM mínima

- Um sistema leve que o cliente de VPN aceite (o Windows que a VPN pede; ou Debian ou Alpine,
  quando a VPN tem cliente Linux).
- Uma placa **host-only** (ou *rede interna do host*) só entre o seu computador e a VM, com um
  IP fixo (por exemplo `192.168.56.11`). Nada de ponte para a rede local: o proxy não deve
  ficar acessível para ninguém além de você.
- A VPN do cliente instalada e configurada dentro da VM.

## O proxy dentro da VM

Escolha um dos dois:

- **`microsocks`** (Linux): `microsocks -i 192.168.56.11 -p 1080`. No perfil: SOCKS5,
  `192.168.56.11`, porta `1080`.
- **`sshd` da VM** (qualquer sistema com OpenSSH): o app já fala SSH. Em vez de um proxy,
  ponha a VM como **servidor de salto** do perfil (Host do salto `192.168.56.11`, o usuário
  e a credencial dela), e cada conexão do grupo sai pelo `sshd` da VM. O salto vale para SSH e
  SFTP, não para Telnet nem FTP.

## Subir e descer

Os dois comandos do perfil falam com o gerenciador da VM:

| Gerenciador | Subir | Descer |
|---|---|---|
| VirtualBox | `VBoxManage startvm cliente-a --type headless` | `VBoxManage controlvm cliente-a acpipowerbutton` |
| libvirt/KVM | `virsh start cliente-a` | `virsh shutdown cliente-a` |
| Multipass | `multipass start cliente-a` | `multipass stop cliente-a` |

A VM leva um tempo para iniciar e a VPN dela, mais um pouco: ponha em **Esperar até (s)** um
valor folgado (120, por exemplo). O app só conecta quando a porta do proxy (ou do `sshd`)
responde.

## Segurança

A VM guarda as credenciais da VPN do cliente: proteja o disco dela como protegeria o
computador. Mantenha a rede host-only: o proxy nunca em ponte.
