"""
runner.py

This script provides a framework for launching and managing virtual machines (VMs) in a simulated network mesh using QEMU. It supports both x86_64 and aarch64 architectures, and automates network setup, VM provisioning, and repository injection via cloud-init and ISO images.

Main components:
    - Network: Manages a virtual bridge, IP allocation, and tap device setup for VMs.
    - Machine: Represents a single VM, handles image creation, cloud-init, launching, and remote actions.
    - Mesh: Orchestrates multiple VMs as a group, with unified repository injection and lifecycle management.

Usage:
    - Can be run as a script to launch a mesh of VMs, or interactively to launch a single VM for testing.
    - Requires root privileges for network and tap device setup.
    - Relies on QEMU, genisoimage, cloud-localds, and other system tools.
"""
import argparse
import os
import shutil
from typing import Literal, Union
import subprocess
from pathlib import Path
import datetime
import time

from click import command

PATH = Path(__file__).parent
BASE_IMAGES_ROOT = PATH / "images"
OVERLAY_IMAGES_ROOT = BASE_IMAGES_ROOT / "overlays"

def build_repo_cdrom(
    root_dir: Path,
    repo_name: str,
    repo_url: str,
    branch_name: str,
    arch: Literal["x86_64", "aarch64"] = "x86_64"
):
    """
    Clone a git repository and package it as a CD-ROM ISO image.
    Used to inject code into VMs for testing or deployment.
    """
    subprocess.run([
        "git", "clone", "--depth", "1", "--branch", branch_name,
        repo_url, str(root_dir / repo_name)
    ], check=True)
    # build release for the target
    target = {
        "x86_64": "x86_64-unknown-linux-gnu",
        "aarch64": "aarch64-unknown-linux-gnu"
    }[arch]
    print(f"Building {repo_name} for target {target}...")
    time.sleep(1)
    subprocess.run([
        "cargo", "build", "--release", "--target", target,
        "--manifest-path", str(root_dir / repo_name / "Cargo.toml")
    ], check=True)
    # genisoimage with the built executable
    iso_path = root_dir / "repo.iso"
    subprocess.run([
        "genisoimage", "-o", str(iso_path), "-V", "REPO", "-r", "-J",
        str(root_dir / repo_name / "target" / target / "release" / repo_name)
    ], check=True)
    return str(iso_path)

class Network:
    """
    Manages a virtual network bridge and tap devices for VMs.
    Handles IP allocation, bridge setup/teardown, and tap device lifecycle.
    """
    def __init__(
        self, 
        gateway: str = "192.168.122.1",
        bridge_name: str = "maxtrixbr0",
        host_uplink: str = "wlo1",
    ):
        """
        Initialize the Network with gateway, bridge name, and host uplink.
        Sets up IP mask and prepares for tap device allocation.
        """
        self.gateway = gateway
        self.mask = gateway.rsplit(".", 1)[0] + ".0"
        self.bridge_name = bridge_name
        self.host_uplink = host_uplink
        self.is_up = False
        self.assigned_ips = set()

    def is_valid_client_ip(self, ip: str) -> bool:
        """
        Returns True if the given IP is valid for a client (not gateway, in subnet).
        """
        if not ip.startswith(self.gateway.rsplit(".", 1)[0] + "."):
            return False
        if ip == self.gateway:
            return False
        return 0 < int(ip.split(".")[-1]) < 255

    def allocate_device(self) -> tuple[str, str, str]:
        """
        Allocates an IP address, creates a tap device, and generates a MAC address for a VM.
        Returns a tuple (ip, tap_name, mac).
        """
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
        last_octet = final_candidate.split(".")[-1]
        mac = f"52:54:00:12:34:{int(last_octet):02x}"
        return final_candidate, tap_name, mac

    def _create_tap(self, ip: str):
        """
        Creates and attaches a tap device for the VM to the bridge.
        Returns the tap device name.
        """
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
    
    def up(self):
        """
        Sets up the network bridge, enables forwarding, and configures iptables for NAT.
        Raises RuntimeError if already up.
        """
        if self.is_up:
            raise RuntimeError("Network is already up.")
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
        self.is_up = True
        return self

    
    def down(self):
        """
        Tears down the network bridge, removes iptables rules, and deletes tap devices.
        Raises RuntimeError if already down.
        """
        if not self.is_up:
            raise RuntimeError("Network is already down.")
        self.is_up = False
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

    def __enter__(self):
        """
        Context manager entry: brings the network up.
        """
        self.up()
        return self

    def __exit__(self, *_):
        """
        Context manager exit: brings the network down.
        """
        self.down()
        
        
class Machine:
    """
    Represents a single VM instance.
    Handles image creation, cloud-init injection, launching, and remote actions via SSH.
    """
    def __init__(
        self,
        arch: Literal["x86_64", "aarch64"],
        network: Network,
        ram: int = 2048,
        cpu_cores: int = 2,
        repo_name: str = "pillar",
        repo_url: str = "git@github.com:aheschl1/pillar.git",
        branch_name: str = "main",
        git_ssh_key: str = "~/.ssh/id_rsa",
        root_dir: Path | None = None,
        repo_cdrom: Path | None = None,
        daemonize: bool = True,
        attach_console: bool = False,
    ):
        self.ip_address, self.tap_name, self.mac = network.allocate_device()
        self.arch: Literal["x86_64", "aarch64"] = arch
        self.ram = ram
        self.cpu_cores = cpu_cores
        self.network = network
        self.repo_name = repo_name
        self.repo_url = repo_url
        self.branch_name = branch_name
        self.git_ssh_key = git_ssh_key
        self.daemonize = daemonize
        self.attach_console = attach_console
        self.repo_cdrom = repo_cdrom
        self.id = f"{arch}-{self.ip_address.replace('.', '-')}-{datetime.datetime.now().strftime('%Y%m%d%H%M%S')}"
        if root_dir is None:
            self.root_dir = OVERLAY_IMAGES_ROOT / self.id
        else:
            self.root_dir = root_dir / self.id
        os.makedirs(self.root_dir, exist_ok=True)

    @property
    def base_image(self):
        """
        Returns the base image filename for the VM architecture.
        """
        return {
            'x86_64': 'debian-12-genericcloud-amd64.qcow2',
            'aarch64': 'debian-12-genericcloud-arm64.qcow2'
        }[self.arch]

    def _inject_user_data(self, user_data_path: Path, output_path: Path):
        """
        Injects SSH keys and VM-specific variables into cloud-init user-data templates.
        """
        with open(os.path.expanduser("~/.ssh/id_rsa.pub"), 'r') as f:
            ssh_pub_key = f.read().strip()
        with open(user_data_path, 'r') as f:
            user_data = f.read().strip()
        with open(os.path.expanduser(self.git_ssh_key), 'r') as f:
            ssh_priv_key = f.read().strip()
        
        net_device_name = {
            "x86_64": "ens3",
            "aarch64": "eth0"
        }[self.arch]
        
        user_data = user_data.replace("<ssh_pub_key>", ssh_pub_key)
        user_data = user_data.replace("<ssh_priv_key>", ssh_priv_key)
        user_data = user_data.replace("<ip_address>", self.ip_address)
        user_data = user_data.replace("<gateway>", self.network.gateway)
        user_data = user_data.replace("<repo_name>", self.repo_name)
        user_data = user_data.replace("<repo_url>", self.repo_url)
        user_data = user_data.replace("<branch_name>", self.branch_name)
        user_data = user_data.replace("<net_device_name>", net_device_name)
        with open(output_path, 'w') as f:
            f.write(user_data)
        
    def build_repo_cdrom(self):
        """
        Build or reuse a repository ISO for VM injection.
        """
        if self.repo_cdrom is not None:
            return str(self.repo_cdrom)
        return build_repo_cdrom(
            root_dir=self.root_dir,
            repo_name=self.repo_name,
            repo_url=self.repo_url,
            branch_name=self.branch_name,
            arch=self.arch
        )
    
    def build_init_iso(self):
        """
        Build a cloud-init ISO for VM initialization (user-data, meta-data, network config).
        """
        user_data = BASE_IMAGES_ROOT / "cloud_init" / "user-data"
        meta_data = BASE_IMAGES_ROOT / "cloud_init" / "meta-data"
        network_plan = BASE_IMAGES_ROOT / "cloud_init" / "network-config"
        self._inject_user_data(user_data, self.root_dir / "user-data")
        self._inject_user_data(network_plan, self.root_dir / "network-config")
        iso_path = self.root_dir / "build-init.iso"
        subprocess.run([
            "cloud-localds",
            f"--network-config={str(self.root_dir / 'network-config')}",
            str(iso_path),
            str(self.root_dir / "user-data"),
            str(meta_data),
        ])
        return str(iso_path)
    
    def ping(self):
        """
        Check if the VM responds to ICMP ping.
        Returns True if responsive.
        """
        ret = subprocess.run([
            "ping", "-c", "1", "-W", "1", self.ip_address
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return ret.returncode == 0

    def build_image(self):
        """
        Create an overlay image for the VM, build cloud-init ISO, and repo ISO.
        Returns paths to all three.
        """
        overlay = f"overlay.qcow2"
        subprocess.run([
            "qemu-img", "create", 
            "-f", "qcow2", 
            "-F", "qcow2",
            "-b", str(BASE_IMAGES_ROOT / self.base_image),
            str(self.root_dir / overlay)
        ], check=True)
        # Make cloud-init ISO
        return str(self.root_dir / overlay), self.build_init_iso(), self.build_repo_cdrom()

    def _launch_command(self):
        """
        Build the QEMU launch command for the VM, including network, drives, and console options.
        Returns the command as a list.
        """
        accel_args = {
            "x86_64": "-enable-kvm -cpu host",
            "aarch64": "-cpu cortex-a57 -M virt -machine accel=tcg"

        }[self.arch]
        net_device = {
            "x86_64": "virtio-net-pci",
            "aarch64": "virtio-net-device"
        }[self.arch]

        image, drive, repo_iso = self.build_image()

        log_path = self.root_dir / f"qemu_{self.arch}_{self.ip_address.replace('.', '_')}.log"

        command = [
            f"qemu-system-{self.arch}",
            "-m", str(self.ram),
            "-smp", str(self.cpu_cores),
            *accel_args.split(),
            "-netdev", f"tap,id=net0,ifname={self.tap_name},script=no,downscript=no",
            "-device", f"{net_device},netdev=net0,mac={self.mac}",
            "-drive", f"file={image},if=virtio,format=qcow2,index=0,media=disk",
        ]

        if self.arch == "x86_64":
            # keep CD-ROM setup for x86_64 as before
            command.extend([
                "-drive", f"file={drive},media=cdrom,index=1",
                "-drive", f"file={repo_iso},media=cdrom,index=2",
            ])
        else:  # aarch64
            command.extend([
                "-bios", "/usr/share/AAVMF/AAVMF_CODE.fd",
                # add virtio-scsi controller
                "-device", "virtio-scsi-pci,id=scsi0",
                # attach init ISO
                "-drive", f"file={drive},if=none,format=raw,id=cdrom0,media=cdrom",
                "-device", "scsi-cd,drive=cdrom0,bus=scsi0.0",
                # attach repo ISO
                "-drive", f"file={repo_iso},if=none,format=raw,id=cdrom1,media=cdrom",
                "-device", "scsi-cd,drive=cdrom1,bus=scsi0.0",
            ])
        if self.daemonize:
            command.append("-daemonize")
        if self.attach_console:
            command.append("-nographic")
        else:
            command.extend([
                "-display", "none",
                "-serial", f"file:{log_path}",
            ])
        print("==================== Launch Command ====================")
        # pretty print command
        print(" \\\n  ".join(command))
        return command
    
    def wait_responsive(self, timeout: float = 300):
        """
        Wait until the VM responds to ping, or timeout.
        Raises TimeoutError if not responsive in time.
        """
        start_time = time.time()
        while True:
            if time.time() - start_time > timeout:
                raise TimeoutError(f"Machine {self.id} did not become responsive in time.")
            if self.ping():
                break
            time.sleep(0.5)
            
    def wait_ssh_responsive(self, timeout: float = 300):
        """
        Wait until the VM accepts SSH connections, or timeout.
        Raises TimeoutError if not responsive in time.
        """
        start_time = time.time()
        while True:
            if time.time() - start_time > timeout:
                raise TimeoutError(f"Machine {self.id} did not become SSH responsive in time.")
            ret = subprocess.run([
                "ssh", "-o", "StrictHostKeyChecking=no", f"root@{self.ip_address}", "echo", "ok"
            ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if ret.returncode == 0:
                break
            time.sleep(0.5)
    
    def wait_setup_complete(self, timeout: float = 600):
        """
        Wait until the VM setup is complete (setup-complete file exists), or timeout.
        Raises TimeoutError if not complete in time.
        """
        start_time = time.time()
        while True:
            if time.time() - start_time > timeout:
                raise TimeoutError(f"Machine {self.id} did not complete setup in time.")
            ret = subprocess.run([
                "ssh", "-o", "StrictHostKeyChecking=no", f"root@{self.ip_address}", "test", "-f", f"/root/setup-complete"
            ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if ret.returncode == 0:
                break
            time.sleep(1)
    
    def launch(self):
        """
        Launch the VM using QEMU and return the process handle.
        """
        return subprocess.Popen(self._launch_command())
    
    def act(self, command: str):
        """
        Execute a shell command on the VM via SSH.
        Raises RuntimeError if VM is not responsive.
        Returns the result of subprocess.run.
        """
        command = command.replace("{ip_address}", self.ip_address)
        if not self.ping():
            raise RuntimeError(f"Machine {self.id} is not responsive.")
        # do not require verification
        ssh_command = ["ssh", "-o", "StrictHostKeyChecking=no", f"root@{self.ip_address}", "bash", "-c", f"'{command}'"]
        return subprocess.run(ssh_command)

class Mesh:
    """
    Orchestrates a group of VMs as a mesh.
    Handles unified repo injection, lifecycle management, and batch operations.
    """
    def __init__(
        self, 
        network: Network | None = None, 
        name: str | None = None, 
        unified_cdrom: Literal["true", "false", "arch"] = "false",
        **kwargs
    ):
        """
        Initialize the Mesh with an optional network, name, and unified CD-ROM option.
        Prepares root directory and mesh state.
        """
        self.kwargs = kwargs
        self.machines: list[Machine] = []
        self.processes: list[subprocess.Popen] = []
        self.network = network or Network()
        self.name = name
        self.cdrom_path: dict[str, Path | None] = {"x86_64": None, "aarch64": None}
        self.unified_cdrom = unified_cdrom
        if self.name:
            self.root_dir = OVERLAY_IMAGES_ROOT / self.name
            if self.root_dir.exists():
                override = input(f"Mesh '{self.name}' already exists. Override? (y/N): ")
                if override.lower() != 'y':
                    raise FileExistsError(f"Mesh '{self.name}' already exists.")
                shutil.rmtree(self.root_dir)
            os.makedirs(self.root_dir, exist_ok=True)
        else:
            self.root_dir = None

    def __enter__(self):
        """
        Context manager entry: brings up the network and builds unified repo CD-ROM if needed.
        """
        if not self.network.is_up:
            self.network.up()
        if self.unified_cdrom in ["true", "arch"]:
            root = (self.root_dir or OVERLAY_IMAGES_ROOT) / "x86_64"
            os.makedirs(root, exist_ok=True)
            self.cdrom_path["x86_64"] = build_repo_cdrom( # type: ignore
                root_dir=root,
                arch="x86_64",
                repo_name=self.kwargs.get("repo_name", "pillar"),
                branch_name=self.kwargs.get("branch_name", "main"),
                repo_url=self.kwargs.get("repo_url", "https://github.com/example/repo.git")
            )
            if self.unified_cdrom == "arch":
                root = (self.root_dir or OVERLAY_IMAGES_ROOT) / "aarch64"
                os.makedirs(root, exist_ok=True)
                self.cdrom_path["aarch64"] = build_repo_cdrom( # type: ignore
                    root_dir=root,
                    arch="aarch64",
                    repo_name=self.kwargs.get("repo_name", "pillar"),
                    branch_name=self.kwargs.get("branch_name", "main"),
                    repo_url=self.kwargs.get("repo_url", "https://github.com/example/repo.git")
                )
            else:
                self.cdrom_path["aarch64"] = self.cdrom_path["x86_64"] # type: ignore
        return self

    def __exit__(self, *_):
        """
        Context manager exit: brings down the network and terminates all VMs.
        """
        if self.network.is_up:
            self.network.down()
        self.terminate_all()
        pass
    
    def enroll_machine(self, arch: Literal["x86_64", "aarch64"], **kwargs) -> Machine:
        """
        Add a new VM to the mesh, optionally using a unified repo ISO.
        Returns the created Machine instance.
        """
        assert self.cdrom_path[arch] is not None, "Unified CD-ROM not built for this architecture."
        repo_cdrom = Path(self.cdrom_path[arch]) if self.unified_cdrom in ["true", "arch"] else None # type: ignore
        machine = Machine(
            arch, 
            self.network, root_dir=self.root_dir, repo_cdrom=repo_cdrom, **kwargs)
        self.machines.append(machine)
        return machine
    
    def wait_responsive(self, timeout: int = 300):
        """
        Wait until all enrolled VMs respond to ping, or timeout.
        """
        start_time = time.time()
        for machine in self.machines:
            machine.wait_responsive(timeout=timeout - (time.time() - start_time))
    
    def wait_ssh_responsive(self, timeout: int = 300):
        """
        Wait until all enrolled VMs accept SSH connections, or timeout.
        """
        start_time = time.time()
        for machine in self.machines:
            machine.wait_ssh_responsive(timeout=timeout - (time.time() - start_time))

    def wait_setup_complete(self, timeout: int = 600):
        """
        Wait until all enrolled VMs complete setup (setup-complete file exists), or timeout.
        """
        start_time = time.time()
        for machine in self.machines:
            machine.wait_setup_complete(timeout=timeout - (time.time() - start_time))

    def act_all(self, command: str):
        """
        Execute a shell command on all enrolled VMs via SSH.
        Raises RuntimeError if any VM is not responsive.
        Returns a list of subprocess.CompletedProcess results.
        """
        results = []
        for machine in self.machines:
            results.append(machine.act(command))
        return results
    
    def launch_all(self):
        """
        Launch all enrolled VMs and store their process handles.
        """
        processes = []
        for machine in self.machines:
            processes.append(machine.launch())
        self.processes = processes
    
    def wait_all(self):
        """
        Wait for all VM processes to exit.
        """
        for proc in self.processes:
            proc.wait()
    
    def terminate_all(self):
        """
        Terminate all VM processes.
        """
        for proc in self.processes:
            proc.terminate()
    
    def kill_all(self):
        """
        Kill all VM processes immediately.
        """
        for proc in self.processes:
            proc.kill()

def parse_args():
    """
    Parse command-line arguments for mesh size and name.
    """
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
    parser.add_argument(
        "--name",
        type=str,
        default=None,
        help="Name for the mesh, used to create a dedicated directory"
    )
    parser.add_argument(
        "--action",
        type=str,
        default="cd /root && ./pillar/pillar -- ip-address={ip_address}",
        help="Command to run machines"
    )
    return parser.parse_args()

def main():
    """
    Main entry point for launching a mesh of VMs from command-line arguments.
    """
    args = parse_args()

    with Mesh(name=args.name, unified_cdrom="arch", repo_url="git@github.com:aheschl1/pillar.git") as mesh:
        for _ in range(args.n_x86):
            mesh.enroll_machine("x86_64")
        for _ in range(args.n_aarch):
            mesh.enroll_machine("aarch64")
        print(f"Launching {len(mesh.machines)} machines...")
        mesh.launch_all()
        print("Waiting for machines to become responsive...")
        mesh.wait_setup_complete()
        print("All machines are setup.")
        # Trigger calls
        if args.action:
            if os.path.exists(args.action):
                with open(args.action, 'r') as f:
                    args.action = f.read().strip()
            print(f"Running command on all machines:\n{args.action}")
            results = mesh.act_all(args.action)
            for i, result in enumerate(results):
                print(f"--- Machine {i} ({mesh.machines[i].ip_address}) ---")
                print(f"Return code: {result.returncode}")
        else:
            print("No command specified, skipping action phase.")
        mesh.terminate_all()
        print("All machines terminated.")
    
if __name__ == "__main__":
    main()
    # cmnd = ' && '.join([
    #     "cd /root",
    #     "./pillar/pillar"
    # ])
    # with Network() as net:
    #     machine = Machine("aarch64", net, daemonize=False, attach_console=True)
    #     proc = machine.launch()
    #     machine.wait_setup_complete()
    #     machine.act(cmnd)
    #     # proc.kill()
    #     proc.wait()
    