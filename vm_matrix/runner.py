import argparse
import os
from typing import Literal
import subprocess
from pathlib import Path
import datetime
import sys

PATH = Path(__file__).parent
BASE_IMAGES_ROOT = PATH / "images"
OVERLAY_IMAGES_ROOT = BASE_IMAGES_ROOT / "overlays"

class Network:
    def __init__(
        self, 
        gateway: str = "192.168.122.1",
        bridge_name: str = "maxtrixbr0",
        host_uplink: str = "wlo1",
    ):
        self.gateway = gateway
        self.mask = gateway.rsplit(".", 1)[0] + ".0"
        self.bridge_name = bridge_name
        self.host_uplink = host_uplink
        self.assigned_ips = set()

    def is_valid_client_ip(self, ip: str) -> bool:
        if not ip.startswith(self.gateway.rsplit(".", 1)[0] + "."):
            return False
        if ip == self.gateway:
            return False
        return 0 < int(ip.split(".")[-1]) < 255
    
    def allocate_device(self) -> str:
        base = self.gateway.rsplit(".", 1)[0] + "."
        final_candidate = None
        for i in range(2, 255):
            candidate = f"{base}{i}"
            if candidate not in self.assigned_ips:
                self.assigned_ips.add(candidate)
                final_candidate = candidate
                break
        if final_candidate is None:
            raise RuntimeError("No available IP addresses.")
        tap_name = self._create_tap(final_candidate)
        return final_candidate, tap_name

    def _create_tap(self, ip: str):
        tap_name = f"tap{ip.split('.')[-1]}"
        subprocess.run([
            "sudo", "ip", "tuntap", "add", tap_name, "mode", "tap"
        ], check=True)
        subprocess.run([
            "sudo", "ip", "link", "set", tap_name, "up"
        ], check=True)
        subprocess.run([
            "sudo", "ip", "link", "set", tap_name, "master", self.bridge_name
        ], check=True)
        return tap_name

    def __enter__(self):
        subprocess.run([
            "sudo", "sysctl", "-w", "net.ipv4.ip_forward=1"
        ], check=True)
        subprocess.run([
            "sudo", "ip", "link", "add", self.bridge_name, "type", "bridge"
        ], check=True)
        subprocess.run([
            "sudo", "ip", "addr", "add", f"{self.gateway}/24", "dev", self.bridge_name
        ], check=True)
        subprocess.run([
            "sudo", "ip", "link", "set", self.bridge_name, "up"
        ], check=True)
        subprocess.run([
            "sudo", "iptables", "-A", "FORWARD", "-i", self.bridge_name, "-j", "ACCEPT"
        ], check=True)
        subprocess.run([
            "sudo", "iptables", "-A", "FORWARD", "-i", self.host_uplink, "-o", self.bridge_name, "-m", "state", "--state", "RELATED,ESTABLISHED", "-j", "ACCEPT"
        ], check=True)
        subprocess.run([
            "sudo", "iptables", "-t", "nat", "-A", "POSTROUTING", "-s", f"{self.mask}/24", "-o", self.host_uplink, "-j", "MASQUERADE"
        ], check=True)
        return self

    def __exit__(self, *_):
        subprocess.run([
            "sudo", "ip", "link", "set", self.bridge_name, "down"
        ], check=True)
        subprocess.run([
            "sudo", "ip", "link", "del", self.bridge_name, "type", "bridge"
        ], check=True)
        # iptables
        subprocess.run([
            "sudo", "iptables", "-D", "FORWARD", "-i", self.bridge_name, "-j", "ACCEPT"
        ], check=True)
        subprocess.run([
            "sudo", "iptables", "-D", "FORWARD", "-i", self.host_uplink, "-o", self.bridge_name, "-m", "state", "--state", "RELATED,ESTABLISHED", "-j", "ACCEPT"
        ], check=True)
        subprocess.run([
            "sudo", "iptables", "-t", "nat", "-D", "POSTROUTING", "-s", f"{self.mask}/24", "-o", self.host_uplink, "-j", "MASQUERADE"
        ], check=True)
        # teardown taps
        for ip in self.assigned_ips:
            tap_name = f"tap{ip.split('.')[-1]}"
            subprocess.run([
                "sudo", "ip", "link", "set", tap_name, "down"
            ], check=True)
            subprocess.run([
                "sudo", "ip", "link", "del", tap_name, "type", "tap"
            ], check=True)
        
class Machine:
    def __init__(
        self, 
        arch: Literal["x86_64", "aarch64"],
        network: Network,
        ram: int=2048,
        cpu_cores: int=2,
        repo_name: str = "pillar",
        repo_url: str = "git@github.com:aheschl1/pillar.git",
        branch_name: str = "main",
        git_ssh_key: str = "~/.ssh/id_rsa",
    ):
        self.ip_address, self.tap_name = network.allocate_device()
        self.arch = arch
        self.ram = ram
        self.cpu_cores = cpu_cores
        self.network = network
        self.repo_name = repo_name
        self.repo_url = repo_url
        self.branch_name = branch_name
        self.git_ssh_key = git_ssh_key
        self.id = f"{arch}-{self.ip_address.replace('.', '-')}-{datetime.datetime.now().strftime('%Y%m%d%H%M%S')}"
        self.root_dir = OVERLAY_IMAGES_ROOT / self.id
        os.makedirs(self.root_dir, exist_ok=True)

    @property
    def base_image(self):
        return {
            'x86_64': 'debian-12-genericcloud-amd64.qcow2',
            'aarch64': 'ubuntu-20.04-aarch64.qcow2'
        }[self.arch]

    def _inject_user_data(self, user_data_path: Path, output_path: Path):
        with open(os.path.expanduser("~/.ssh/id_rsa.pub"), 'r') as f:
            ssh_pub_key = f.read().strip()
        with open(user_data_path, 'r') as f:
            user_data = f.read().strip()
        with open(os.path.expanduser(self.git_ssh_key), 'r') as f:
            ssh_priv_key = f.read().strip()
        user_data = user_data.replace("<ssh_pub_key>", ssh_pub_key)
        user_data = user_data.replace("<ssh_priv_key>", ssh_priv_key)
        user_data = user_data.replace("<ip_address>", self.ip_address)
        user_data = user_data.replace("<gateway>", self.network.gateway)
        user_data = user_data.replace("<repo_name>", self.repo_name)
        user_data = user_data.replace("<repo_url>", self.repo_url)
        user_data = user_data.replace("<branch_name>", self.branch_name)
        with open(output_path, 'w') as f:
            f.write(user_data)

    def build_image(self):
        overlay = f"overlay.qcow2"
        drive = f"cloud-init.iso"
        subprocess.run([
            "qemu-img", "create", 
            "-f", "qcow2", 
            "-F", "qcow2",
            "-b", str(BASE_IMAGES_ROOT / self.base_image),
            str(self.root_dir / overlay)
        ], check=True)
        # Make cloud-init ISO
        user_data = BASE_IMAGES_ROOT / "cloud_init" / "user-data"
        meta_data = BASE_IMAGES_ROOT / "cloud_init" / "meta-data"
        network_plan = BASE_IMAGES_ROOT / "cloud_init" / "network-config"
        self._inject_user_data(user_data, self.root_dir / "user-data")
        self._inject_user_data(network_plan, self.root_dir / "network-config")
        subprocess.run([
            "cloud-localds",
            f"--network-config={str(self.root_dir / 'network-config')}",
            str(self.root_dir / drive),
            str(self.root_dir / "user-data"),
            str(meta_data),
        ])
        return str(self.root_dir / overlay), str(self.root_dir / drive)

    def _launch_command(self):
        accel_args = {
            "x86_64": "-enable-kvm -cpu host",
            "aarch64": "-cpu cortex-a57 -M virt"
        }[self.arch]
        device = {
            "x86_64": "virtio-net-pci",
            "aarch64": "virtio-net-device"
        }[self.arch]

        image, drive = self.build_image()

        return [
            f"qemu-system-{self.arch}",
            "-m", str(self.ram),
            "-smp", str(self.cpu_cores),
            *accel_args.split(),
            "-drive", f"file={image},if=virtio,format=qcow2",
            "-netdev", f"tap,id=net0,ifname={self.tap_name},script=no,downscript=no",
            "-device", f"{device},netdev=net0",
            "-cdrom", f"{drive}",
            "-nographic"
        ]
        
    
    def launch(self):
        return subprocess.Popen(self._launch_command())

class Matrix:
    def __init__(self):
        self.machines = []

    def enroll_machine(self, arch):
        print(f"Enrolling a {arch} machine.")

def parse_args():
    parser = argparse.ArgumentParser(description="VM Matrix Runner")
    parser.add_argument(
        "--n-x86",
        type=int,
        default=0,
        help="Number of x86_64 machines to launch"
    )
    parser.add_argument(
        "--n-aarch",
        type=int,
        default=0,
        help="Number of aarch64 machines to launch"
    )
    return parser.parse_args()

def main():
    args = parse_args()
    print(f"Launching {args.n_x86} x86_64 machines and {args.n_aarch} aarch64 machines.")
    
if __name__ == "__main__":
    # main()
    with Network() as net:
        machine = Machine("x86_64", net)
        print("Launching machine...")
        # machine._launch_command()
        pid = machine.launch()
        pid.wait()